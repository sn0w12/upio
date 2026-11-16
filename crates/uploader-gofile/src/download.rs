//! GoFile download resolution.
//!
//! GoFile's API needs a guest account token and a short-lived website token.
//! The website token is recomputed from the account token, a 4-hour window,
//! and a salt embedded in GoFile's client script. Direct store links also need
//! the account token as a cookie.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use reqwest::header::{ACCEPT, AUTHORIZATION, COOKIE, ORIGIN, RANGE, REFERER, USER_AGENT};
use reqwest::{Client, Response, Url};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use upio_core::download::{Downloader, ParsedUrl, RemoteFile, RemoteNode, ResourceKind};
use upio_core::error::UpioError;
use upio_core::http::map_reqwest_error;
use upio_core::service::DownloadDef;
use upio_core::types::UploaderEndpointConfig;

const ACCOUNTS_URL: &str = "https://api.gofile.io/accounts";
const CONTENT_URL: &str = "https://api.gofile.io/contents";
const WEBSITE_ORIGIN: &str = "https://gofile.io";
const WT_SCRIPT_URL: &str = "https://gofile.io/js/wt.obf.js";
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                                AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 \
                                Safari/537.36";
const LANGUAGE: &str = "en-US";

/// Fallback salt, used only when the live client script cannot be decoded.
/// The preferred path extracts the current salt from `wt.obf.js`.
const FALLBACK_WEBSITE_TOKEN_SALT: &str = "12af056dacea0b";
const WEBSITE_TOKEN_WINDOW_SECONDS: i64 = 14_400;

const MAX_FOLDER_DEPTH: usize = 64;
const RATE_LIMIT_ATTEMPTS: usize = 4;

#[derive(Debug, Deserialize)]
struct AccountEnvelope {
    status: String,
    #[serde(default)]
    data: Option<AccountData>,
}

#[derive(Debug, Deserialize)]
struct AccountData {
    token: String,
}

#[derive(Debug, Deserialize)]
struct ContentsEnvelope {
    status: String,
    #[serde(default)]
    data: Option<ContentData>,
}

#[derive(Debug, Deserialize)]
struct ContentData {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    children: HashMap<String, ContentChild>,
    #[serde(default)]
    contents: HashMap<String, ContentChild>,
    #[serde(rename = "passwordStatus", default)]
    password_status: Option<String>,
    #[serde(rename = "canAccess", default)]
    can_access: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ContentChild {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

pub struct GoFileDownloader {
    client: Client,
    provided_token: Option<String>,
    guest_token: RwLock<Option<String>>,
    salt: RwLock<Option<String>>,
}

impl GoFileDownloader {
    pub const NAME: &str = "gofile";

    pub fn new() -> Self {
        Self {
            client: Client::new(),
            provided_token: None,
            guest_token: RwLock::new(None),
            salt: RwLock::new(None),
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self {
            client: Client::new(),
            provided_token: Some(token.to_string()),
            guest_token: RwLock::new(None),
            salt: RwLock::new(None),
        }
    }

    async fn token(&self) -> Result<String, UpioError> {
        if let Some(token) = &self.provided_token {
            return Ok(token.clone());
        }
        if let Some(token) = self.guest_token.read().await.clone() {
            return Ok(token);
        }

        let mut guard = self.guest_token.write().await;
        if let Some(token) = guard.clone() {
            return Ok(token);
        }
        let token = self.create_guest().await?;
        *guard = Some(token.clone());
        Ok(token)
    }

    async fn create_guest(&self) -> Result<String, UpioError> {
        let response = self
            .client
            .post(ACCOUNTS_URL)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ORIGIN, WEBSITE_ORIGIN)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        let envelope: AccountEnvelope = response.json().await.map_err(map_reqwest_error)?;
        if envelope.status != "ok" {
            return Err(api_error(&envelope.status));
        }
        let token = envelope
            .data
            .map(|data| data.token)
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| UpioError {
                message: "GoFile created a guest account but returned no token".to_string(),
                status_code: Some(status.as_u16()),
            })?;
        Ok(token)
    }

    async fn salt(&self) -> String {
        if let Some(salt) = self.salt.read().await.clone() {
            return salt;
        }

        let mut guard = self.salt.write().await;
        if let Some(salt) = guard.clone() {
            return salt;
        }

        let salt = self
            .extract_salt()
            .await
            .unwrap_or_else(|| FALLBACK_WEBSITE_TOKEN_SALT.to_string());
        *guard = Some(salt.clone());
        salt
    }

    async fn extract_salt(&self) -> Option<String> {
        let response = self
            .client
            .get(WT_SCRIPT_URL)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let script = response.text().await.ok()?;
        extract_salt_from_script(&script)
    }

    async fn website_token(&self, account_token: &str, window_offset: i64) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        website_token_for(
            account_token,
            now / WEBSITE_TOKEN_WINDOW_SECONDS + window_offset,
            USER_AGENT_VALUE,
            LANGUAGE,
            &self.salt().await,
        )
    }

    fn fetch_node<'a>(
        &'a self,
        token: &'a str,
        id: &'a str,
        password: Option<&'a str>,
        depth: usize,
    ) -> Pin<Box<dyn Future<Output = Result<RemoteNode, UpioError>> + Send + 'a>> {
        Box::pin(async move {
            if depth > MAX_FOLDER_DEPTH {
                return Err(UpioError {
                    message: "GoFile folder nesting is too deep".to_string(),
                    status_code: None,
                });
            }

            let data = self.contents(token, id, password).await?;
            let ContentData {
                kind,
                name,
                link,
                size,
                children,
                contents,
                ..
            } = data;

            if kind == "file" {
                let link =
                    link.filter(|link| !link.trim().is_empty())
                        .ok_or_else(|| UpioError {
                            message: "GoFile returned a file without a download link".to_string(),
                            status_code: None,
                        })?;
                let mut file = RemoteFile::new(display_name(name, id), link);
                file.size = size;
                return Ok(RemoteNode::File(file));
            }

            let mut nodes = Vec::new();
            for (child_id, child) in if children.is_empty() {
                contents
            } else {
                children
            } {
                if child.kind == "folder" {
                    nodes.push(
                        self.fetch_node(token, &child_id, password, depth + 1)
                            .await?,
                    );
                } else if let Some(file) = file_from_child(child, &child_id) {
                    nodes.push(RemoteNode::File(file));
                }
            }

            Ok(RemoteNode::Folder {
                name: display_name(name, "gofile"),
                children: nodes,
            })
        })
    }

    async fn contents(
        &self,
        token: &str,
        id: &str,
        password: Option<&str>,
    ) -> Result<ContentData, UpioError> {
        let mut window_offset = 0i64;

        for attempt in 0..RATE_LIMIT_ATTEMPTS {
            let website_token = self.website_token(token, window_offset).await;
            let mut query = vec![
                ("contentFilter", ""),
                ("page", "1"),
                ("pageSize", "1000"),
                ("sortField", "createTime"),
                ("sortDirection", "-1"),
            ];
            if let Some(password) = password {
                query.push(("password", password));
            }

            let response = self
                .client
                .get(format!("{CONTENT_URL}/{id}"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header("X-Website-Token", website_token)
                .header("X-BL", LANGUAGE)
                .header(USER_AGENT, USER_AGENT_VALUE)
                .header(ACCEPT, "*/*")
                .header(ORIGIN, WEBSITE_ORIGIN)
                .header(REFERER, "https://gofile.io/")
                .query(&query)
                .send()
                .await
                .map_err(map_reqwest_error)?;

            let http_status = response.status();
            let body = response.text().await.map_err(map_reqwest_error)?;
            let envelope: ContentsEnvelope =
                serde_json::from_str(&body).map_err(|error| UpioError {
                    message: format!("unreadable GoFile response: {error}"),
                    status_code: Some(http_status.as_u16()),
                })?;

            if envelope.status == "ok" {
                let data = envelope.data.ok_or_else(|| UpioError {
                    message: "GoFile returned an empty content response".to_string(),
                    status_code: Some(http_status.as_u16()),
                })?;
                if data
                    .password_status
                    .as_deref()
                    .is_some_and(|status| status != "passwordOk")
                {
                    return Err(UpioError {
                        message: "GoFile content is password protected".to_string(),
                        status_code: None,
                    });
                }
                if data.can_access == Some(false) {
                    return Err(UpioError {
                        message: "GoFile content is not accessible".to_string(),
                        status_code: None,
                    });
                }
                return Ok(data);
            }

            match envelope.status.as_str() {
                "error-rateLimit" if attempt + 1 < RATE_LIMIT_ATTEMPTS => {
                    let wait = 3 * (attempt as u64 + 1);
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
                "error-notPremium" if window_offset == 0 => {
                    window_offset = -1;
                }
                _ => return Err(api_error(&envelope.status)),
            }
        }

        Err(api_error("error-rateLimit"))
    }
}

impl Default for GoFileDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Downloader for GoFileDownloader {
    fn client(&self) -> &Client {
        &self.client
    }

    async fn init(&self) -> Result<(), UpioError> {
        self.token().await.map(|_| ())
    }

    async fn resolve(
        &self,
        url: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError> {
        let parsed = Url::parse(url).map_err(|error| UpioError {
            message: format!("invalid GoFile URL '{url}': {error}"),
            status_code: None,
        })?;
        let id = content_id(&parsed).ok_or_else(|| UpioError {
            message: format!("'{url}' is not a GoFile share URL"),
            status_code: None,
        })?;
        let password = password_hash(&parsed);
        let token = self.token().await?;
        let node = self.fetch_node(&token, id, password.as_deref(), 0).await?;
        Ok(flatten_single_file(node))
    }

    async fn open(
        &self,
        file: &RemoteFile,
        offset: u64,
        _config: &UploaderEndpointConfig,
    ) -> Result<Response, UpioError> {
        let token = self.token().await?;
        let mut request = self
            .client
            .get(&file.url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "*/*")
            .header(REFERER, "https://gofile.io/")
            .header(COOKIE, format!("accountToken={token}"));
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }

        let response = request.send().await.map_err(map_reqwest_error)?;
        if response
            .url()
            .host_str()
            .is_some_and(|host| host == "gofile.io" || host == "www.gofile.io")
        {
            return Err(UpioError {
                message: "GoFile returned its website instead of the file; the download link or \
                          account token expired"
                    .to_string(),
                status_code: Some(response.status().as_u16()),
            });
        }
        Ok(response)
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

pub fn parse_url(url: &str) -> Option<ParsedUrl> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if host != "gofile.io" && host != "www.gofile.io" {
        return None;
    }
    let id = content_id(&parsed)?;
    Some(ParsedUrl {
        id: id.to_string(),
        kind: ResourceKind::Collection,
    })
}

fn content_id(url: &Url) -> Option<&str> {
    let mut segments = url.path_segments()?;
    if segments.next()? != "d" {
        return None;
    }
    segments.next().filter(|id| !id.is_empty())
}

fn password_hash(url: &Url) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == "password")
        .filter(|(_, value)| !value.is_empty())
        .map(|(_, value)| hex_sha256(value.as_bytes()))
}

fn file_from_child(child: ContentChild, id: &str) -> Option<RemoteFile> {
    let link = child.link.filter(|link| !link.trim().is_empty())?;
    let name = display_name(child.name, id);
    let mut file = RemoteFile::new(name, link);
    file.size = child.size;
    Some(file)
}

fn display_name(name: String, fallback: &str) -> String {
    if name.trim().is_empty() {
        fallback.to_string()
    } else {
        name
    }
}

fn flatten_single_file(node: RemoteNode) -> RemoteNode {
    match node {
        RemoteNode::Folder { name, mut children } if children.len() == 1 => {
            match children.remove(0) {
                RemoteNode::File(file) => RemoteNode::File(file),
                other => RemoteNode::Folder {
                    name,
                    children: vec![other],
                },
            }
        }
        other => other,
    }
}

fn website_token_for(
    account_token: &str,
    window: i64,
    user_agent: &str,
    language: &str,
    salt: &str,
) -> String {
    hex_sha256(format!("{user_agent}::{language}::{account_token}::{window}::{salt}").as_bytes())
}

fn hex_sha256(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

/// Recover the website-token salt from GoFile's obfuscated client script.
///
/// The salt is one entry in an encrypted string table. The table is rotated by
/// the script's bootstrap, so every rotation is tried and the decoded values
/// are filtered to the salt's shape.
fn extract_salt_from_script(script: &str) -> Option<String> {
    let array = string_arrays(script).into_iter().max_by_key(Vec::len)?;
    if array.len() < 8 {
        return None;
    }

    let body = function_body(script, "generateWT")?;
    let calls = decoder_calls(body);
    if calls.is_empty() {
        return None;
    }

    for rotation in 0..array.len() {
        for (index, key) in &calls {
            if *index >= array.len() {
                continue;
            }
            let entry = &array[(index + rotation) % array.len()];
            let Some(value) = decode_entry(entry, key) else {
                continue;
            };
            if is_salt(&value) {
                return Some(value);
            }
        }
    }

    None
}

fn is_salt(value: &str) -> bool {
    value.len() == 14
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn string_arrays(script: &str) -> Vec<Vec<String>> {
    let mut arrays = Vec::new();
    let mut offset = 0;

    while let Some(relative) = script[offset..].find('[') {
        let start = offset + relative;
        if !script[..start].trim_end().ends_with('=') {
            offset = start + 1;
            continue;
        }
        if let Some(end) = matching_bracket(script, start) {
            let values = array_strings(&script[start + 1..end]);
            if values.len() >= 8 {
                arrays.push(values);
            }
            offset = end + 1;
        } else {
            offset = start + 1;
        }
    }

    arrays
}

fn matching_bracket(text: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = start;

    while index < text.len() {
        let ch = text[index..].chars().next()?;
        match ch {
            '[' => depth += 1,
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            '\'' | '"' => {
                index = skip_string(text, index)?;
                continue;
            }
            _ => {}
        }
        index += ch.len_utf8();
    }

    None
}

fn array_strings(content: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0;

    while index < content.len() {
        let ch = content[index..].chars().next().unwrap_or('\0');
        if ch == '\'' || ch == '"' {
            if let Some((value, end)) = parse_string_literal(content, index) {
                values.push(value);
                index = end;
                continue;
            }
        }
        index += ch.len_utf8();
    }

    values
}

fn function_body<'a>(script: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("function {name}(");
    let start = script.find(&marker)?;
    let open = script[start..].find('{')? + start;

    let mut depth = 0usize;
    let mut index = open;
    while index < script.len() {
        let ch = script[index..].chars().next()?;
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&script[open..=index]);
                }
            }
            '\'' | '"' => {
                index = skip_string(script, index)?;
                continue;
            }
            _ => {}
        }
        index += ch.len_utf8();
    }

    None
}

fn decoder_calls(body: &str) -> Vec<(usize, String)> {
    let mut calls = Vec::new();
    let mut offset = 0;

    while let Some(relative) = body[offset..].find("0x") {
        let start = offset + relative + 2;
        let digits = body[start..]
            .chars()
            .take_while(|ch| ch.is_ascii_hexdigit())
            .collect::<String>();
        if digits.len() < 2 {
            offset = start;
            continue;
        }

        let after = start + digits.len();
        let Some(rest) = body[after..].trim_start().strip_prefix(',') else {
            offset = after;
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('\'') && !rest.starts_with('"') {
            offset = after;
            continue;
        }

        let quote = body.len() - rest.len();
        if let Some((key, _)) = parse_string_literal(body, quote) {
            if let Ok(index) = usize::from_str_radix(&digits, 16) {
                if let Some(index) = index.checked_sub(0x136) {
                    calls.push((index, key));
                }
            }
        }

        offset = after;
    }

    calls
}

fn skip_string(text: &str, start: usize) -> Option<usize> {
    parse_string_literal(text, start).map(|(_, end)| end)
}

fn parse_string_literal(text: &str, start: usize) -> Option<(String, usize)> {
    let quote = text[start..].chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let mut value = String::new();
    let mut index = start + quote.len_utf8();

    while index < text.len() {
        let ch = text[index..].chars().next()?;
        if ch == quote {
            return Some((value, index + ch.len_utf8()));
        }
        if ch != '\\' {
            value.push(ch);
            index += ch.len_utf8();
            continue;
        }

        index += 1;
        let escaped = text[index..].chars().next()?;
        match escaped {
            'x' => {
                let hex = text.get(index + 1..index + 3)?;
                value.push(u8::from_str_radix(hex, 16).ok()? as char);
                index += 3;
            }
            'u' => {
                let hex = text.get(index + 1..index + 5)?;
                let code = u32::from_str_radix(hex, 16).ok()?;
                value.push(char::from_u32(code)?);
                index += 5;
            }
            'n' => {
                value.push('\n');
                index += 1;
            }
            'r' => {
                value.push('\r');
                index += 1;
            }
            't' => {
                value.push('\t');
                index += 1;
            }
            'b' => {
                value.push('\u{8}');
                index += 1;
            }
            'f' => {
                value.push('\u{c}');
                index += 1;
            }
            'v' => {
                value.push('\u{b}');
                index += 1;
            }
            '0' => {
                value.push('\0');
                index += 1;
            }
            other => {
                value.push(other);
                index += other.len_utf8();
            }
        }
    }

    None
}

fn custom_base64(value: &str) -> Vec<u8> {
    const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/=";

    let mut bits = 0u64;
    let mut count = 0u32;
    let mut output = Vec::new();

    for ch in value.chars() {
        let Some(index) = ALPHABET.find(ch) else {
            continue;
        };
        if index >= 64 {
            continue;
        }
        bits = ((bits << 6) | index as u64) & 0xFFFF_FFFF;
        count += 6;
        if count >= 8 {
            count -= 8;
            output.push(((bits >> count) & 0xFF) as u8);
        }
    }

    output
}

fn decode_entry(entry: &str, key: &str) -> Option<String> {
    let cipher = String::from_utf8(custom_base64(entry)).ok()?;
    let key_units = key.encode_utf16().collect::<Vec<_>>();
    if key_units.is_empty() {
        return None;
    }

    let mut state: [u8; 256] = std::array::from_fn(|index| index as u8);
    let mut j = 0usize;
    for i in 0..256 {
        j = (j + state[i] as usize + key_units[i % key_units.len()] as usize) % 256;
        state.swap(i, j);
    }

    let mut i = 0usize;
    j = 0;
    let mut output = String::new();
    for unit in cipher.encode_utf16() {
        i = (i + 1) % 256;
        j = (j + state[i] as usize) % 256;
        state.swap(i, j);
        let keystream = state[(state[i] as usize + state[j] as usize) % 256];
        output.push(char::from_u32((unit ^ keystream as u16) as u32)?);
    }

    Some(output)
}

fn api_error(status: &str) -> UpioError {
    let message = match status {
        "error-notPremium" => {
            "GoFile rejected the website token; its embedded salt may have changed".to_string()
        }
        "error-rateLimit" => "GoFile rate-limited the request".to_string(),
        "error-notFound" => "GoFile content was not found or has been removed".to_string(),
        "error-passwordRequired" => "GoFile content needs the correct password".to_string(),
        other => other
            .split_once('-')
            .map(|(_, message)| message)
            .unwrap_or(other)
            .to_string(),
    };
    UpioError {
        message,
        status_code: None,
    }
}

pub static GOFILE_DOWNLOAD: DownloadDef = DownloadDef {
    parse: parse_url,
    credential: upio_core::Credential::OptionalToken,
    build,
};

fn build(
    config: &UploaderEndpointConfig,
) -> Result<upio_core::BoxedDownloader, upio_core::BuildError> {
    Ok(Box::new(match config.token.as_deref() {
        Some(token) if !token.trim().is_empty() => GoFileDownloader::with_token(token),
        _ => GoFileDownloader::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_urls_are_detected() {
        assert_eq!(
            parse_url("https://gofile.io/d/abc123").unwrap().id,
            "abc123"
        );
        assert_eq!(
            parse_url("https://www.gofile.io/d/abc123").unwrap().id,
            "abc123"
        );
        assert!(parse_url("https://api.gofile.io/d/abc123").is_none());
        assert!(parse_url("https://example.com/d/abc123").is_none());
        assert!(parse_url("https://gofile.io/other/abc123").is_none());
    }

    #[test]
    fn the_website_token_matches_the_client_algorithm() {
        assert_eq!(
            website_token_for(
                "abc",
                123,
                USER_AGENT_VALUE,
                "en-US",
                FALLBACK_WEBSITE_TOKEN_SALT
            ),
            "4a83b913f073b9b03e7bd8f23fefd438f3e769c9522124e166aa9cd7a9193fcf"
        );
    }

    #[test]
    fn the_obfuscated_salt_entry_decodes() {
        assert_eq!(
            decode_entry("W5iiW6BcINeap8ofW484x8oxW50Q", "]6)]").as_deref(),
            Some("12af056dacea0b")
        );
    }

    #[test]
    fn the_salt_is_extracted_from_an_obfuscated_script() {
        let script = r#"
            (function (a, b) {}(F, 1));
            function F() {
                var x = [
                    'W5iiW6BcINeap8ofW484x8oxW50Q',
                    'a', 'b', 'c', 'd', 'e', 'f', 'g'
                ];
                return x;
            }
            function generateWT(t) {
                var d = function (i, k) { return ''; };
                return d(0x136, '\x5d\x36\x29\x5d');
            }
        "#;

        assert_eq!(
            extract_salt_from_script(script).as_deref(),
            Some("12af056dacea0b")
        );
    }

    #[tokio::test]
    #[ignore = "fetches GoFile's live client script"]
    async fn extracts_the_live_salt() {
        let downloader = GoFileDownloader::new();
        assert!(downloader.extract_salt().await.is_some());
    }

    #[test]
    fn a_password_query_is_hashed() {
        let url = Url::parse("https://gofile.io/d/abc123?password=secret").unwrap();
        assert_eq!(
            password_hash(&url).as_deref(),
            Some("2bb80d537b1da3e38bd30361aa855686bde0eacd7162fef6a25fe97bf527a25b")
        );
    }

    #[test]
    fn a_single_file_folder_is_flattened() {
        let node = RemoteNode::Folder {
            name: "abc123".to_string(),
            children: vec![RemoteNode::File(RemoteFile::new(
                "a.txt",
                "https://store.example/a",
            ))],
        };
        assert!(
            matches!(flatten_single_file(node), RemoteNode::File(file) if file.name == "a.txt")
        );
    }
}
