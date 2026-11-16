//! GoonBox download resolution.
//!
//! GoonBox serves public pages as a React shell. The data comes from JSON
//! endpoints: single images, album pages, and profile image listings. An API
//! key or an account bearer reaches link-only albums and images; the keyed API
//! lives under `/api/v1`, the session API under `/api`. Originals live on a
//! separate CDN that requires a GoonBox referer and supports range requests.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::header::{HeaderValue, AUTHORIZATION, REFERER, USER_AGENT};
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::Value;

use crate::login::{Credentials, Session};
use upio_core::download::{Downloader, ParsedUrl, RemoteFile, RemoteNode, ResourceKind};
use upio_core::error::UpioError;
use upio_core::http::map_reqwest_error;
use upio_core::service::DownloadDef;
use upio_core::types::UploaderEndpointConfig;

const BASE_URL: &str = "https://goonbox.cr";
const API_BASE: &str = "https://goonbox.cr/api";
const API_V1_BASE: &str = "https://goonbox.cr/api/v1";
const USER_PAGE_SIZE: u32 = 100;
const MAX_REDIRECTS: usize = 3;

const DESKTOP_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                                  AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 \
                                  Safari/537.36";

#[derive(Debug, Deserialize)]
struct ImageResponse {
    image: Option<Image>,
}

#[derive(Debug, Deserialize)]
struct AlbumResponse {
    album: Option<Album>,
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    pagination: Option<Pagination>,
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    user: Option<User>,
}

#[derive(Debug, Deserialize)]
struct ImagesResponse {
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    pagination: Option<Pagination>,
}

#[derive(Debug, Deserialize)]
struct Image {
    #[serde(default)]
    encoded_id: String,
    #[serde(default)]
    original_filename: Option<String>,
    #[serde(default)]
    original_url: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct Album {
    #[serde(default)]
    encoded_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct User {
    #[serde(default)]
    username: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Pagination {
    #[serde(default)]
    current_page: u32,
    #[serde(default)]
    last_page: u32,
}

pub struct GoonboxDownloader {
    client: Client,
    api_key: Option<String>,
    session: Option<Arc<Session>>,
}

impl GoonboxDownloader {
    pub const NAME: &str = "goonbox";

    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_key: None,
            session: None,
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: Some(token.to_string()),
            session: None,
        }
    }

    pub fn with_account(username: &str, password: &str) -> Self {
        let credentials = Credentials {
            username: username.to_string(),
            password: password.to_string(),
            solved_captcha: tokio::sync::RwLock::new(None),
            profile_dir: crate::default_profile_dir(),
        };
        let session = Session::new(credentials).unwrap_or_else(|_| Session::new_fallback());
        Self {
            client: Client::new(),
            api_key: None,
            session: Some(Arc::new(session)),
        }
    }

    fn http(&self) -> &Client {
        self.session
            .as_ref()
            .map(|session| session.client())
            .unwrap_or(&self.client)
    }

    fn api_base(&self) -> &'static str {
        if self.api_key.is_some() {
            API_V1_BASE
        } else {
            API_BASE
        }
    }

    async fn authorization(&self) -> Result<Option<HeaderValue>, UpioError> {
        let bearer = if let Some(api_key) = &self.api_key {
            api_key.clone()
        } else if let Some(session) = &self.session {
            session
                .bearer()
                .await
                .map_err(|error| UpioError {
                    message: error.message,
                    status_code: error.status_code,
                })?
                .to_string()
        } else {
            return Ok(None);
        };

        HeaderValue::from_str(&format!("Bearer {bearer}"))
            .map(Some)
            .map_err(|error| UpioError {
                message: format!("GoonBox returned an unusable bearer: {error}"),
                status_code: None,
            })
    }

    async fn fetch_json(&self, path: &str) -> Result<Value, UpioError> {
        let primary = format!("{}{path}", self.api_base());
        match self.fetch_json_url(&primary).await {
            Err(error) if self.api_key.is_some() && error.status_code == Some(404) => {
                self.fetch_json_url(&format!("{API_BASE}{path}")).await
            }
            result => result,
        }
    }

    async fn fetch_json_url(&self, url: &str) -> Result<Value, UpioError> {
        let auth_first = self.api_key.is_some();
        match self.fetch_json_url_with(url, auth_first).await {
            Err(error)
                if !auth_first
                    && self.session.is_some()
                    && matches!(error.status_code, Some(401 | 403 | 404)) =>
            {
                self.fetch_json_url_with(url, true).await
            }
            result => result,
        }
    }

    async fn fetch_json_url_with(&self, url: &str, authorize: bool) -> Result<Value, UpioError> {
        let mut current = url.to_string();

        for _ in 0..MAX_REDIRECTS {
            let mut request = self
                .http()
                .get(&current)
                .headers(crate::login::browser_headers());
            if authorize {
                if let Some(auth) = self.authorization().await? {
                    request = request.header(AUTHORIZATION, auth);
                }
            }
            let response = request.send().await.map_err(map_reqwest_error)?;
            let status = response.status();
            let body = response.text().await.map_err(map_reqwest_error)?;

            if crate::login::is_cloudflare_challenge(status.as_u16(), &body) {
                return Err(UpioError {
                    message: "Cloudflare blocked the GoonBox request".to_string(),
                    status_code: Some(status.as_u16()),
                });
            }

            let value: Value = serde_json::from_str(&body).map_err(|error| UpioError {
                message: format!("GoonBox returned no JSON for {current}: {error}"),
                status_code: Some(status.as_u16()),
            })?;

            if let Some(redirect) = value.get("redirect").and_then(Value::as_str) {
                current = Url::parse(&current)
                    .and_then(|base| base.join(redirect))
                    .map(|url| url.to_string())
                    .map_err(|error| UpioError {
                        message: format!("GoonBox returned an invalid redirect: {error}"),
                        status_code: Some(status.as_u16()),
                    })?;
                continue;
            }

            if !status.is_success() {
                return Err(api_error(&value, status.as_u16()));
            }
            return Ok(value);
        }

        Err(UpioError {
            message: format!("GoonBox redirected too many times for {url}"),
            status_code: None,
        })
    }

    fn remote_file(&self, image: &Image) -> Result<RemoteFile, UpioError> {
        let url = image
            .original_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| UpioError {
                message: format!("GoonBox image '{}' has no original URL", image.encoded_id),
                status_code: None,
            })?;
        let name = image
            .original_filename
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .or_else(|| filename_from_url(url))
            .unwrap_or_else(|| image.encoded_id.clone());

        let mut file = RemoteFile::new(name, url);
        file.size = image.size_bytes;
        file.headers
            .push((REFERER.as_str().to_string(), format!("{BASE_URL}/")));
        file.headers.push((
            USER_AGENT.as_str().to_string(),
            DESKTOP_USER_AGENT.to_string(),
        ));
        Ok(file)
    }

    fn images(&self, images: &[Image]) -> Result<Vec<RemoteNode>, UpioError> {
        images
            .iter()
            .map(|image| self.remote_file(image).map(RemoteNode::File))
            .collect()
    }
}

impl Default for GoonboxDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Downloader for GoonboxDownloader {
    fn client(&self) -> &Client {
        &self.client
    }

    async fn resolve(
        &self,
        url: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError> {
        let parsed = Url::parse(url).map_err(|error| UpioError {
            message: format!("invalid GoonBox URL '{url}': {error}"),
            status_code: None,
        })?;

        match target(&parsed) {
            Some(Target::Image(id)) => {
                let value = self.fetch_json(&format!("/images/{id}")).await?;
                let response: ImageResponse =
                    serde_json::from_value(value).map_err(|error| UpioError {
                        message: format!("unreadable GoonBox image response: {error}"),
                        status_code: None,
                    })?;
                let image = response.image.ok_or_else(|| UpioError {
                    message: format!("GoonBox image '{id}' is unavailable"),
                    status_code: None,
                })?;
                self.remote_file(&image).map(RemoteNode::File)
            }
            Some(Target::Album(id)) => self.resolve_album(&id).await,
            Some(Target::User(username)) => self.resolve_user(&username).await,
            None => Err(UpioError {
                message: format!("'{url}' is not a GoonBox image, album, or profile URL"),
                status_code: None,
            }),
        }
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

impl GoonboxDownloader {
    async fn resolve_album(&self, id: &str) -> Result<RemoteNode, UpioError> {
        let mut path = format!("/albums/{id}");
        let mut canonical = id.to_string();
        let mut name = None;
        let mut children = Vec::new();

        loop {
            let value = self.fetch_json(&path).await?;
            let response: AlbumResponse =
                serde_json::from_value(value).map_err(|error| UpioError {
                    message: format!("unreadable GoonBox album response: {error}"),
                    status_code: None,
                })?;

            if name.is_none() {
                let album = response.album.ok_or_else(|| UpioError {
                    message: format!("GoonBox album '{id}' is unavailable"),
                    status_code: None,
                })?;
                canonical = album
                    .encoded_id
                    .filter(|id| !id.trim().is_empty())
                    .unwrap_or(canonical);
                name = Some(
                    album
                        .title
                        .filter(|title| !title.trim().is_empty())
                        .unwrap_or_else(|| format!("goonbox-{id}")),
                );
            }

            children.extend(self.images(&response.images)?);

            let pagination = response.pagination.unwrap_or_default();
            if pagination.current_page >= pagination.last_page {
                break;
            }
            path = format!(
                "/albums/{canonical}/images?page={}",
                pagination.current_page + 1
            );
        }

        folder(
            name.unwrap_or_else(|| format!("goonbox-{id}")),
            children,
            id,
        )
    }

    async fn resolve_user(&self, username: &str) -> Result<RemoteNode, UpioError> {
        let value = self.fetch_json(&format!("/users/{username}")).await?;
        let response: UserResponse = serde_json::from_value(value).map_err(|error| UpioError {
            message: format!("unreadable GoonBox profile response: {error}"),
            status_code: None,
        })?;
        let canonical = response
            .user
            .and_then(|user| user.username)
            .filter(|username| !username.trim().is_empty())
            .unwrap_or_else(|| username.to_string());

        let mut page = 1;
        let mut children = Vec::new();

        loop {
            let path = format!("/users/{canonical}/images?page={page}&per_page={USER_PAGE_SIZE}");
            let value = self.fetch_json(&path).await?;
            let response: ImagesResponse =
                serde_json::from_value(value).map_err(|error| UpioError {
                    message: format!("unreadable GoonBox profile images: {error}"),
                    status_code: None,
                })?;

            children.extend(self.images(&response.images)?);

            let pagination = response.pagination.unwrap_or_default();
            if pagination.current_page >= pagination.last_page {
                break;
            }
            page = pagination.current_page + 1;
        }

        folder(canonical, children, username)
    }
}

fn folder(name: String, children: Vec<RemoteNode>, id: &str) -> Result<RemoteNode, UpioError> {
    if children.is_empty() {
        return Err(UpioError {
            message: format!("GoonBox '{id}' has no public images"),
            status_code: None,
        });
    }
    Ok(RemoteNode::Folder { name, children })
}

/// Classify a GoonBox share URL.
///
/// `img` is a single image, `a` is an album, and `u` is a public profile.
/// Album and image IDs may carry the old jpg6 slug prefix, separated by a dot;
/// only the part after the last dot is used.
pub fn parse_url(url: &str) -> Option<ParsedUrl> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if host != "goonbox.cr" && host != "www.goonbox.cr" {
        return None;
    }

    let (id, kind) = match target(&parsed)? {
        Target::Image(id) => (id, ResourceKind::File),
        Target::Album(id) | Target::User(id) => (id, ResourceKind::Collection),
    };
    Some(ParsedUrl { id, kind })
}

enum Target {
    Image(String),
    Album(String),
    User(String),
}

fn target(url: &Url) -> Option<Target> {
    let mut segments = url.path_segments()?;
    let kind = segments.next()?;
    let id = segments.next().filter(|id| !id.is_empty())?;
    match kind {
        "img" => Some(Target::Image(strip_slug(id).to_string())),
        "a" => Some(Target::Album(strip_slug(id).to_string())),
        "u" => Some(Target::User(id.to_string())),
        _ => None,
    }
}

fn strip_slug(id: &str) -> &str {
    id.rsplit_once('.')
        .map(|(_, id)| id)
        .filter(|id| !id.is_empty())
        .unwrap_or(id)
}

fn filename_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()?
        .path_segments()?
        .next_back()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn api_error(value: &Value, status_code: u16) -> UpioError {
    UpioError {
        message: value
            .get("message")
            .and_then(Value::as_str)
            .filter(|message| !message.trim().is_empty())
            .unwrap_or("GoonBox rejected the request")
            .to_string(),
        status_code: Some(status_code),
    }
}

pub static GOONBOX_DOWNLOAD: DownloadDef = DownloadDef {
    parse: parse_url,
    credential: upio_core::Credential::OptionalToken,
    build,
};

fn build(
    config: &UploaderEndpointConfig,
) -> Result<upio_core::BoxedDownloader, upio_core::BuildError> {
    if let Some(token) = config
        .token
        .as_deref()
        .filter(|token| !token.trim().is_empty())
    {
        return Ok(Box::new(GoonboxDownloader::with_token(token)));
    }

    if let (Some(username), Some(password)) = (
        config
            .username
            .as_deref()
            .filter(|username| !username.trim().is_empty()),
        config
            .password
            .as_deref()
            .filter(|password| !password.trim().is_empty()),
    ) {
        return Ok(Box::new(GoonboxDownloader::with_account(
            username, password,
        )));
    }

    Ok(Box::new(GoonboxDownloader::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_urls_are_detected() {
        assert_eq!(
            parse_url("https://goonbox.cr/img/abc123").unwrap().kind,
            ResourceKind::File
        );
        assert_eq!(
            parse_url("https://www.goonbox.cr/a/abc123").unwrap().kind,
            ResourceKind::Collection
        );
        assert_eq!(parse_url("https://goonbox.cr/u/alice").unwrap().id, "alice");
        assert!(parse_url("https://goonbox.cr/api/images/abc123").is_none());
        assert!(parse_url("https://example.com/img/abc123").is_none());
    }

    #[test]
    fn an_api_key_switches_to_the_versioned_api() {
        assert_eq!(GoonboxDownloader::new().api_base(), API_BASE);
        assert_eq!(GoonboxDownloader::with_token("key").api_base(), API_V1_BASE);
    }

    #[test]
    fn old_slug_ids_are_reduced_to_the_encoded_id() {
        assert_eq!(
            parse_url("https://goonbox.cr/a/photo-album.abc123")
                .unwrap()
                .id,
            "abc123"
        );
        assert_eq!(
            parse_url("https://goonbox.cr/img/photo.abc123").unwrap().id,
            "abc123"
        );
        assert_eq!(strip_slug("plain"), "plain");
        assert_eq!(strip_slug("trailing."), "trailing.");
    }

    #[test]
    fn a_file_name_prefers_the_original_name_and_falls_back_to_the_url() {
        let downloader = GoonboxDownloader::new();
        let named = Image {
            encoded_id: "abc".to_string(),
            original_filename: Some("photo.jpg".to_string()),
            original_url: Some("https://cdn.example/other.webp".to_string()),
            size_bytes: Some(42),
        };
        let file = downloader.remote_file(&named).unwrap();
        assert_eq!(file.name, "photo.jpg");
        assert_eq!(file.size, Some(42));
        assert!(file.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("referer") && value == "https://goonbox.cr/"
        }));

        let unnamed = Image {
            encoded_id: "abc".to_string(),
            original_filename: None,
            original_url: Some("https://cdn.example/dir/photo.webp".to_string()),
            size_bytes: None,
        };
        let file = downloader.remote_file(&unnamed).unwrap();
        assert_eq!(file.name, "photo.webp");
    }
}
