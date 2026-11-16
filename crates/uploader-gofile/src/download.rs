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
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                                AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 \
                                Safari/537.36";
const LANGUAGE: &str = "en-US";

/// Salt embedded in GoFile's `wt.obf.js`. If downloads start returning
/// `error-notPremium`, this value has rotated and needs updating.
const WEBSITE_TOKEN_SALT: &str = "12af056dacea0b";
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
}

impl GoFileDownloader {
    pub const NAME: &str = "gofile";

    pub fn new() -> Self {
        Self {
            client: Client::new(),
            provided_token: None,
            guest_token: RwLock::new(None),
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self {
            client: Client::new(),
            provided_token: Some(token.to_string()),
            guest_token: RwLock::new(None),
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
            let website_token = website_token(token, window_offset);
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

fn website_token(account_token: &str, window_offset: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    website_token_for(
        account_token,
        now / WEBSITE_TOKEN_WINDOW_SECONDS + window_offset,
        USER_AGENT_VALUE,
        LANGUAGE,
        WEBSITE_TOKEN_SALT,
    )
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
            website_token_for("abc", 123, USER_AGENT_VALUE, "en-US", WEBSITE_TOKEN_SALT),
            "4a83b913f073b9b03e7bd8f23fefd438f3e769c9522124e166aa9cd7a9193fcf"
        );
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
