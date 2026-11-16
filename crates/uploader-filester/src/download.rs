//! Filester download resolution.
//!
//! Public file and folder pages sit in front of a short-lived download-token
//! endpoint. Folder links can be password protected: the unlock is a form POST
//! whose password field is a base64 payload bound to a nonce, and the resulting
//! cookie authorizes both the folder listing and its files.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::header::{
    HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, ORIGIN, RANGE, REFERER, USER_AGENT,
};
use reqwest::{Client, Response, Url};
use serde::Deserialize;

use upio_core::download::{Downloader, ParsedUrl, RemoteFile, RemoteNode, ResourceKind};
use upio_core::error::UpioError;
use upio_core::http::map_reqwest_error;
use upio_core::service::DownloadDef;
use upio_core::types::UploaderEndpointConfig;

const TOKEN_URL: &str = "https://filester.me/v2/api/public/download";
const PAGE_ORIGIN: &str = "https://filester.me";
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                                AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 \
                                Safari/537.36";
const FALLBACK_SERVER: &str = "https://cn1.filester.me";
const MAX_FOLDER_DEPTH: usize = 16;
const MAX_FOLDER_PAGES: u32 = 10_000;

#[derive(Debug, Deserialize)]
struct DownloadToken {
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug)]
struct FolderFile {
    name: String,
    slug: String,
    size: Option<u64>,
}

#[derive(Debug)]
struct FolderLink {
    id: String,
}

#[derive(Debug)]
struct FolderPage {
    name: Option<String>,
    files: Vec<FolderFile>,
    folders: Vec<FolderLink>,
    last_page: u32,
}

pub struct FilesterDownloader {
    client: Client,
    token: Option<String>,
}

impl FilesterDownloader {
    pub const NAME: &str = "filester";

    pub fn new() -> Self {
        Self {
            client: build_client(),
            token: None,
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self {
            client: build_client(),
            token: Some(token.to_string()),
        }
    }

    fn auth(&self) -> Option<HeaderValue> {
        self.token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .and_then(|token| HeaderValue::from_str(&format!("Bearer {token}")).ok())
    }

    async fn token(&self, slug: &str, page: &Url) -> Result<DownloadToken, UpioError> {
        let mut request = self
            .client
            .post(TOKEN_URL)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/json, text/plain, */*")
            .header(CONTENT_TYPE, "application/json")
            .header(ORIGIN, PAGE_ORIGIN)
            .header(REFERER, page.as_str());
        if let Some(auth) = self.auth() {
            request = request.header(AUTHORIZATION, auth);
        }

        let response = request
            .json(&serde_json::json!({ "file_slug": slug }))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = response.status();
        let body = response.text().await.map_err(map_reqwest_error)?;
        let token: DownloadToken = serde_json::from_str(&body).map_err(|error| UpioError {
            message: format!("Filester returned no download token: {error}"),
            status_code: Some(status.as_u16()),
        })?;

        if !token.success {
            return Err(UpioError {
                message: token
                    .message
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| "Filester refused to issue a download token".to_string()),
                status_code: Some(status.as_u16()),
            });
        }
        Ok(token)
    }

    async fn get_page(&self, url: &str) -> Result<(Url, String), UpioError> {
        let mut request = self
            .client
            .get(url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(
                ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header(ORIGIN, PAGE_ORIGIN);
        if let Some(auth) = self.auth() {
            request = request.header(AUTHORIZATION, auth);
        }

        let response = request.send().await.map_err(map_reqwest_error)?;
        let status = response.status();
        let final_url = response.url().clone();
        let body = response.text().await.map_err(map_reqwest_error)?;
        if !status.is_success() {
            return Err(UpioError {
                message: format!("the Filester page returned {status}"),
                status_code: Some(status.as_u16()),
            });
        }
        Ok((final_url, body))
    }

    async fn folder_page(
        &self,
        url: Url,
        password: Option<&str>,
    ) -> Result<(Url, String), UpioError> {
        let (final_url, html) = self.get_page(url.as_str()).await?;
        let Some(nonce) = password_nonce(&html) else {
            if html.contains("This folder is private") && !html.contains("class=\"file-item\"") {
                return Err(UpioError {
                    message: "Filester folder is private; use an account token with access"
                        .to_string(),
                    status_code: None,
                });
            }
            return Ok((final_url, html));
        };

        let password = password.ok_or_else(|| UpioError {
            message: "Filester folder is password protected; add ?password= to the URL or set \
                      filester.link_password"
                .to_string(),
            status_code: None,
        })?;
        let encoded = encode_password(password, &nonce);

        let mut request = self
            .client
            .post(final_url.as_str())
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ORIGIN, PAGE_ORIGIN)
            .header(REFERER, final_url.as_str())
            .form(&[("password", encoded), ("nonce", nonce)]);
        if let Some(auth) = self.auth() {
            request = request.header(AUTHORIZATION, auth);
        }

        let response = request.send().await.map_err(map_reqwest_error)?;
        let status = response.status();
        let final_url = response.url().clone();
        let html = response.text().await.map_err(map_reqwest_error)?;
        if !status.is_success() {
            return Err(UpioError {
                message: format!("Filester rejected the folder password with {status}"),
                status_code: Some(status.as_u16()),
            });
        }
        if password_nonce(&html).is_some() {
            return Err(UpioError {
                message: "Filester rejected the folder password".to_string(),
                status_code: Some(status.as_u16()),
            });
        }
        Ok((final_url, html))
    }

    fn resolve_folder<'a>(
        &'a self,
        url: Url,
        password: Option<&'a str>,
        depth: usize,
        visited: Arc<Mutex<HashSet<String>>>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<RemoteNode>, UpioError>> + Send + 'a>> {
        Box::pin(async move {
            if depth > MAX_FOLDER_DEPTH {
                return Err(UpioError {
                    message: "Filester folder nesting is too deep".to_string(),
                    status_code: None,
                });
            }

            let Some(id) = folder_id(&url).map(str::to_string) else {
                return Err(UpioError {
                    message: format!("Filester folder URL carries no id: {url}"),
                    status_code: None,
                });
            };
            {
                let mut visited = visited
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if !visited.insert(id.clone()) {
                    return Ok(None);
                }
            }

            let (final_url, html) = self.folder_page(url, password).await?;
            let first = parse_folder(&html);
            let name = first
                .name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| format!("filester-{id}"));

            let mut children = folder_files(&first.files);
            let last_page = first.last_page.min(MAX_FOLDER_PAGES);
            for page in 2..=last_page {
                let (_, html) = self
                    .folder_page(page_url(&final_url, page), password)
                    .await?;
                children.extend(folder_files(&parse_folder(&html).files));
            }

            for folder in first.folders {
                let child_url = folder_url(&folder.id);
                if let Some(node) = self
                    .resolve_folder(child_url, password, depth + 1, visited.clone())
                    .await?
                {
                    children.push(node);
                }
            }

            if children.is_empty() {
                return Ok(None);
            }
            Ok(Some(RemoteNode::Folder { name, children }))
        })
    }
}

impl Default for FilesterDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Downloader for FilesterDownloader {
    fn client(&self) -> &Client {
        &self.client
    }

    async fn resolve(
        &self,
        url: &str,
        config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError> {
        let parsed = Url::parse(url).map_err(|error| UpioError {
            message: format!("invalid Filester URL '{url}': {error}"),
            status_code: None,
        })?;
        let password = password_from_url(&parsed).or_else(|| {
            config
                .link_password
                .as_deref()
                .filter(|password| !password.is_empty())
                .map(str::to_string)
        });

        match target(&parsed) {
            Some(Target::File(slug)) => {
                let page = clean_page_url(parsed);
                match self.token(&slug, &page).await {
                    Ok(token) => Ok(RemoteNode::File(remote_file(&token, &page))),
                    Err(token_error) if token_error.status_code == Some(403) => {
                        let (final_url, _) = self.get_page(page.as_str()).await?;
                        let Some(folder) = folder_id(&final_url) else {
                            return Err(token_error);
                        };
                        let node = self
                            .resolve_folder(
                                folder_url(folder),
                                password.as_deref(),
                                0,
                                Arc::new(Mutex::new(HashSet::new())),
                            )
                            .await?
                            .ok_or_else(|| UpioError {
                                message: format!("Filester folder '{folder}' contained no files"),
                                status_code: None,
                            })?;
                        Ok(find_file(&node, &slug)
                            .map(RemoteNode::File)
                            .unwrap_or(node))
                    }
                    Err(error) => Err(error),
                }
            }
            Some(Target::Folder(id)) => self
                .resolve_folder(
                    folder_url(&id),
                    password.as_deref(),
                    0,
                    Arc::new(Mutex::new(HashSet::new())),
                )
                .await?
                .ok_or_else(|| UpioError {
                    message: format!("Filester folder '{id}' contained no files"),
                    status_code: None,
                }),
            None => Err(UpioError {
                message: format!("'{url}' is not a Filester file or folder URL"),
                status_code: None,
            }),
        }
    }

    async fn open(
        &self,
        file: &RemoteFile,
        offset: u64,
        _config: &UploaderEndpointConfig,
    ) -> Result<Response, UpioError> {
        let page = Url::parse(&file.url).map_err(|error| UpioError {
            message: format!("invalid Filester file URL '{}': {error}", file.url),
            status_code: None,
        })?;
        let slug = slug(&page).ok_or_else(|| UpioError {
            message: format!("Filester file URL carries no slug: {}", file.url),
            status_code: None,
        })?;

        let token = self.token(slug, &page).await?;
        let direct = direct_url(&token)?;

        let mut request = self
            .client
            .get(direct)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "*/*")
            .header(ORIGIN, PAGE_ORIGIN)
            .header(REFERER, page.as_str());
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }

        request.send().await.map_err(map_reqwest_error)
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

/// Classify a Filester share URL.
///
/// Public shares are `/d/<slug>` files and `/f/<id>` folders. Account and CDN
/// hosts are not share URLs.
pub fn parse_url(url: &str) -> Option<ParsedUrl> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if host != "filester.me" && host != "www.filester.me" {
        return None;
    }
    match target(&parsed)? {
        Target::File(slug) => Some(ParsedUrl {
            id: slug,
            kind: ResourceKind::File,
        }),
        Target::Folder(id) => Some(ParsedUrl {
            id,
            kind: ResourceKind::Collection,
        }),
    }
}

enum Target {
    File(String),
    Folder(String),
}

fn target(url: &Url) -> Option<Target> {
    let mut segments = url.path_segments()?;
    let kind = segments.next()?;
    let id = segments.next().filter(|id| !id.is_empty())?;
    match kind {
        "d" => Some(Target::File(id.to_string())),
        "f" => Some(Target::Folder(id.to_string())),
        _ => None,
    }
}

fn slug(url: &Url) -> Option<&str> {
    let mut segments = url.path_segments()?;
    if segments.next()? != "d" {
        return None;
    }
    segments.next().filter(|slug| !slug.is_empty())
}

fn folder_id(url: &Url) -> Option<&str> {
    let mut segments = url.path_segments()?;
    if segments.next()? != "f" {
        return None;
    }
    segments.next().filter(|id| !id.is_empty())
}

fn folder_url(id: &str) -> Url {
    Url::parse(&format!("{PAGE_ORIGIN}/f/{id}")).expect("the Filester origin is a valid URL")
}

fn page_url(base: &Url, page: u32) -> Url {
    let mut url = base.clone();
    url.query_pairs_mut()
        .clear()
        .append_pair("page", &page.to_string());
    url
}

fn clean_page_url(mut url: Url) -> Url {
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(name, _)| name != "password" && name != "page")
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    url
}

fn password_from_url(url: &Url) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == "password")
        .filter(|(_, value)| !value.is_empty())
        .map(|(_, value)| value.into_owned())
}

fn remote_file(token: &DownloadToken, page: &Url) -> RemoteFile {
    let name = token
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .or(token.file.as_deref())
        .unwrap_or("filester-file")
        .to_string();
    RemoteFile::new(name, page.to_string())
}

fn folder_files(files: &[FolderFile]) -> Vec<RemoteNode> {
    files
        .iter()
        .filter(|file| !file.slug.is_empty())
        .map(|file| {
            let name = if file.name.trim().is_empty() {
                file.slug.clone()
            } else {
                file.name.clone()
            };
            let mut remote = RemoteFile::new(name, format!("{PAGE_ORIGIN}/d/{}", file.slug));
            remote.size = file.size;
            RemoteNode::File(remote)
        })
        .collect()
}

fn find_file(node: &RemoteNode, slug: &str) -> Option<RemoteFile> {
    match node {
        RemoteNode::File(file) if file.url.ends_with(&format!("/d/{slug}")) => Some(file.clone()),
        RemoteNode::Folder { children, .. } => {
            children.iter().find_map(|child| find_file(child, slug))
        }
        _ => None,
    }
}

fn parse_folder(html: &str) -> FolderPage {
    FolderPage {
        name: meta_content(html, "og:title")
            .or_else(|| page_title(html))
            .filter(|name| !name.trim().is_empty()),
        files: parse_files(html),
        folders: parse_subfolders(html),
        last_page: last_page(html),
    }
}

fn parse_files(html: &str) -> Vec<FolderFile> {
    let marker = "class=\"file-item\"";
    html.split(marker)
        .skip(1)
        .filter_map(|chunk| {
            let slug = path_id(chunk, "/d/")?;
            let name = attr(chunk, "data-name")
                .map(|name| decode_entities(&name))
                .or_else(|| inner_text(chunk, "file-name"))
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| slug.clone());
            let size = attr(chunk, "data-size").and_then(|size| size.parse().ok());
            Some(FolderFile { name, slug, size })
        })
        .collect()
}

fn parse_subfolders(html: &str) -> Vec<FolderLink> {
    let marker = "subfolder-item";
    let mut folders = Vec::new();
    let mut rest = html;

    while let Some(index) = rest.find(marker) {
        let before = &rest[..index];
        let start = before.rfind('<').unwrap_or(0);
        let tag = &rest[start..];
        let end = tag.find('>').unwrap_or(tag.len());
        if let Some(id) = path_id(&tag[..end], "/f/") {
            folders.push(FolderLink { id });
        }
        rest = &rest[index + marker.len()..];
    }

    folders
}

fn last_page(html: &str) -> u32 {
    let mut last = 1u32;
    let mut rest = html;

    while let Some(index) = rest.find("page=") {
        let after = &rest[index + "page=".len()..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(page) = digits.parse::<u32>() {
            last = last.max(page);
        }
        rest = after;
    }

    if let Some(index) = html.find("data-total=\"") {
        let after = &html[index + "data-total=\"".len()..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(total) = digits.parse::<u32>() {
            last = last.max(total);
        }
    }

    last.min(MAX_FOLDER_PAGES)
}

fn password_nonce(html: &str) -> Option<String> {
    let index = html.find("name=\"nonce\"")?;
    let rest = &html[index..];
    let value = rest.find("value=\"")? + "value=\"".len();
    let end = rest[value..].find('"')? + value;
    Some(rest[value..end].to_string())
}

fn encode_password(password: &str, nonce: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    BASE64.encode(format!("{password}|{timestamp}|{nonce}"))
}

fn path_id(text: &str, prefix: &str) -> Option<String> {
    let start = text.find(prefix)? + prefix.len();
    let id = text[start..]
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>();
    (!id.is_empty()).then_some(id)
}

fn attr(text: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    let start = text.find(&marker)? + marker.len();
    let end = text[start..].find('"')? + start;
    Some(text[start..end].to_string())
}

fn inner_text(text: &str, class: &str) -> Option<String> {
    let marker = format!("class=\"{class}\"");
    let start = text.find(&marker)?;
    let open = text[start..].find('>')? + start;
    let close = text[open + 1..].find('<')? + open + 1;
    Some(decode_entities(text[open + 1..close].trim()))
}

fn meta_content(html: &str, property: &str) -> Option<String> {
    let marker = format!("property=\"{property}\"");
    let index = html.find(&marker)?;
    let rest = &html[index + marker.len()..];
    let content = rest.find("content=\"")? + "content=\"".len();
    let end = rest[content..].find('"')? + content;
    Some(decode_entities(&rest[content..end]))
}

fn page_title(html: &str) -> Option<String> {
    let start = html.find("<title>")? + "<title>".len();
    let end = html[start..].find("</title>")? + start;
    let title = decode_entities(html[start..end].trim());
    title
        .split(" | filester.me")
        .next()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn direct_url(token: &DownloadToken) -> Result<Url, UpioError> {
    let server = token
        .server
        .as_deref()
        .filter(|server| !server.trim().is_empty())
        .unwrap_or(FALLBACK_SERVER);
    let file = token
        .file
        .as_deref()
        .filter(|file| !file.trim().is_empty())
        .ok_or_else(|| UpioError {
            message: "Filester returned a download token without a file".to_string(),
            status_code: None,
        })?;
    let value = token
        .token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| UpioError {
            message: "Filester returned a download token without a token".to_string(),
            status_code: None,
        })?;

    let base = format!(
        "{}/v2/{}",
        server.trim_end_matches('/'),
        file.trim_start_matches('/')
    );
    let mut url = Url::parse(&base).map_err(|error| UpioError {
        message: format!("Filester returned an invalid media URL: {error}"),
        status_code: None,
    })?;

    {
        let mut query = url.query_pairs_mut();
        query.append_pair("token", value);
        query.append_pair("download", "true");
        if let Some(name) = token.name.as_deref().filter(|name| !name.is_empty()) {
            query.append_pair("n", name);
        }
    }

    Ok(url)
}

pub static FILESTER_DOWNLOAD: DownloadDef = DownloadDef {
    parse: parse_url,
    credential: upio_core::Credential::OptionalToken,
    build,
};

fn build_client() -> Client {
    Client::builder()
        .cookie_store(true)
        .build()
        .unwrap_or_default()
}

fn build(
    config: &UploaderEndpointConfig,
) -> Result<upio_core::BoxedDownloader, upio_core::BuildError> {
    Ok(Box::new(match config.token.as_deref() {
        Some(token) if !token.trim().is_empty() => FilesterDownloader::with_token(token),
        _ => FilesterDownloader::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn share_urls_are_detected() {
        assert_eq!(
            parse_url("https://filester.me/d/abc123").unwrap().kind,
            ResourceKind::File
        );
        assert_eq!(
            parse_url("https://www.filester.me/d/abc123").unwrap().id,
            "abc123"
        );
        assert_eq!(
            parse_url("https://filester.me/f/folder1").unwrap().kind,
            ResourceKind::Collection
        );
        assert!(parse_url("https://filester.me/api/v1/upload").is_none());
        assert!(parse_url("https://filester.me/d/").is_none());
        assert!(parse_url("https://example.com/d/abc123").is_none());
        assert!(parse_url("https://u1.filester.me/d/abc123").is_none());
    }

    #[test]
    fn a_share_password_is_read_from_the_query() {
        let url = Url::parse("https://filester.me/f/folder1?password=s%20ecret").unwrap();
        assert_eq!(password_from_url(&url).as_deref(), Some("s ecret"));
        let clean = clean_page_url(url);
        assert!(clean.query().is_none());
    }

    #[test]
    fn folder_markup_yields_files_folders_and_pages() {
        let html = r#"
            <meta property="og:title" content="My &amp; Folder">
            <a href="/f/folder2" class="subfolder-item">
              <div class="subfolder-name">Nested</div>
            </a>
            <div class="file-item" data-name="a &amp; b.txt" data-size="12"
                 onclick="window.location.href='/d/abc123'"></div>
            <button id="loadAllPagesBtn" data-total="3"></button>
        "#;
        let page = parse_folder(html);
        assert_eq!(page.name.as_deref(), Some("My & Folder"));
        assert_eq!(page.last_page, 3);
        assert_eq!(page.files.len(), 1);
        assert_eq!(page.files[0].name, "a & b.txt");
        assert_eq!(page.files[0].slug, "abc123");
        assert_eq!(page.files[0].size, Some(12));
        assert_eq!(page.folders.len(), 1);
        assert_eq!(page.folders[0].id, "folder2");
    }

    #[test]
    fn a_password_nonce_is_extracted_and_encoded() {
        let html = r#"<input type="hidden" id="nonce" name="nonce" value="abc123">"#;
        assert_eq!(password_nonce(html).as_deref(), Some("abc123"));
        let encoded = encode_password("secret", "abc123");
        let decoded = String::from_utf8(BASE64.decode(encoded).unwrap()).unwrap();
        let parts = decoded.split('|').collect::<Vec<_>>();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "secret");
        assert_eq!(parts[2], "abc123");
    }

    #[test]
    fn a_file_inside_a_tree_is_found_by_slug() {
        let node = RemoteNode::Folder {
            name: "root".to_string(),
            children: vec![
                RemoteNode::File(RemoteFile::new("a", "https://filester.me/d/abc123")),
                RemoteNode::Folder {
                    name: "sub".to_string(),
                    children: vec![RemoteNode::File(RemoteFile::new(
                        "b",
                        "https://filester.me/d/def456",
                    ))],
                },
            ],
        };
        assert_eq!(find_file(&node, "def456").unwrap().name, "b");
        assert!(find_file(&node, "missing").is_none());
    }

    #[test]
    fn the_direct_url_uses_the_token_server_and_name() {
        let token = DownloadToken {
            success: true,
            message: None,
            server: Some("https://cdn.example".to_string()),
            file: Some("123.txt".to_string()),
            token: Some("secret".to_string()),
            name: Some("a b.txt".to_string()),
        };

        let url = direct_url(&token).unwrap();
        assert_eq!(url.host_str(), Some("cdn.example"));
        assert_eq!(url.path(), "/v2/123.txt");

        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query.get("token").map(String::as_str), Some("secret"));
        assert_eq!(query.get("download").map(String::as_str), Some("true"));
        assert_eq!(query.get("n").map(String::as_str), Some("a b.txt"));
    }

    #[test]
    fn a_missing_server_falls_back_to_the_public_cdn() {
        let token = DownloadToken {
            success: true,
            message: None,
            server: None,
            file: Some("123.txt".to_string()),
            token: Some("secret".to_string()),
            name: None,
        };
        let url = direct_url(&token).unwrap();
        assert_eq!(url.host_str(), Some("cn1.filester.me"));
    }
}
