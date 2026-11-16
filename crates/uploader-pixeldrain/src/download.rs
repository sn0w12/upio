//! Pixeldrain download resolution.
//!
//! Files resolve through the public JSON API and download from the official
//! range-capable endpoint. Pixeldrain throttles free downloads by IP, so the
//! bypass proxy pool used by the community userscript is tried first for fresh
//! transfers. Partial transfers use the official endpoint, which honors ranges.

use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::header::{
    HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, RANGE, REFERER, USER_AGENT,
};
use reqwest::{Client, Response, Url};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

use upio_core::download::{Downloader, ParsedUrl, RemoteFile, RemoteNode, ResourceKind};
use upio_core::error::UpioError;
use upio_core::http::map_reqwest_error;
use upio_core::service::DownloadDef;
use upio_core::types::UploaderEndpointConfig;

const BASE_URL: &str = "https://pixeldrain.com";
const FILE_API: &str = "https://pixeldrain.com/api/file";
const LIST_API: &str = "https://pixeldrain.com/api/list";
const PROXY_POOL_URL: &str = "https://pixeldrain-bypass.gamedrive.org/api/proxy.json";
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                                AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 \
                                Safari/537.36";

const DEFAULT_PROXIES: &[&str] = &[
    "https://pixeldrain-bypass.gamedrive.org/api/file/",
    "https://pixeldrainbypass.org/api/file/",
    "https://cdn.pixeldrain.eu.cc/",
];

#[derive(Debug, Deserialize)]
struct FileInfo {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ListDetail {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    files: Option<Vec<ListFile>>,
}

#[derive(Debug, Deserialize)]
struct ListFile {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: Option<u64>,
}

pub struct PixeldrainDownloader {
    client: Client,
    api_key: Option<String>,
    proxies: RwLock<Option<Vec<String>>>,
}

impl PixeldrainDownloader {
    pub const NAME: &str = "pixeldrain";

    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_key: None,
            proxies: RwLock::new(None),
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: Some(token.to_string()),
            proxies: RwLock::new(None),
        }
    }

    fn auth(&self) -> Option<HeaderValue> {
        self.api_key
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .map(|token| {
                let encoded = BASE64.encode(format!(":{token}"));
                HeaderValue::from_str(&format!("Basic {encoded}"))
                    .expect("base64 output is a valid header value")
            })
    }

    async fn proxies(&self) -> Vec<String> {
        if let Some(proxies) = self.proxies.read().await.clone() {
            return proxies;
        }

        let mut guard = self.proxies.write().await;
        if let Some(proxies) = guard.clone() {
            return proxies;
        }
        let proxies = self.fetch_proxies().await;
        *guard = Some(proxies.clone());
        proxies
    }

    async fn fetch_proxies(&self) -> Vec<String> {
        let fetched = async {
            let response = self
                .client
                .get(PROXY_POOL_URL)
                .header(USER_AGENT, USER_AGENT_VALUE)
                .timeout(Duration::from_secs(3))
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?;
            response.text().await.ok()
        }
        .await;

        let mut proxies = fetched
            .and_then(|body| serde_json::from_str::<Value>(&body).ok())
            .map(|value| {
                let raw = value
                    .get("proxies")
                    .and_then(Value::as_array)
                    .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                    .or_else(|| {
                        value
                            .get("proxy")
                            .and_then(Value::as_str)
                            .map(|proxy| vec![proxy])
                    })
                    .or_else(|| {
                        value.as_array().map(|values| {
                            values.iter().filter_map(Value::as_str).collect::<Vec<_>>()
                        })
                    })
                    .unwrap_or_default();
                raw.into_iter()
                    .filter_map(normalize_proxy)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if proxies.is_empty() {
            proxies = DEFAULT_PROXIES
                .iter()
                .map(|proxy| proxy.to_string())
                .collect();
        }
        proxies
    }

    async fn info(&self, id: &str) -> Result<FileInfo, UpioError> {
        let response = self
            .client
            .get(format!("{FILE_API}/{id}/info"))
            .headers(self.api_headers())
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = response.status();
        let value: Value = response.json().await.map_err(map_reqwest_error)?;
        if value.get("success") != Some(&Value::Bool(true)) {
            return Err(api_error(&value, Some(status.as_u16())));
        }
        serde_json::from_value(value).map_err(|error| UpioError {
            message: format!("unreadable Pixeldrain file info: {error}"),
            status_code: Some(status.as_u16()),
        })
    }

    async fn list(&self, id: &str) -> Result<ListDetail, UpioError> {
        let response = self
            .client
            .get(format!("{LIST_API}/{id}"))
            .headers(self.api_headers())
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = response.status();
        let value: Value = response.json().await.map_err(map_reqwest_error)?;
        if value.get("success") == Some(&Value::Bool(false)) {
            return Err(api_error(&value, Some(status.as_u16())));
        }
        serde_json::from_value(value).map_err(|error| UpioError {
            message: format!("unreadable Pixeldrain list: {error}"),
            status_code: Some(status.as_u16()),
        })
    }

    fn api_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(USER_AGENT, USER_AGENT_VALUE.parse().unwrap());
        headers.insert(ACCEPT, "application/json".parse().unwrap());
        if let Some(auth) = self.auth() {
            headers.insert(AUTHORIZATION, auth);
        }
        headers
    }

    async fn proxy_response(&self, id: &str, offset: u64, file_name: &str) -> Option<Response> {
        for proxy in self.proxies().await {
            let mut request = self
                .client
                .get(format!("{proxy}{id}"))
                .header(USER_AGENT, USER_AGENT_VALUE)
                .header(ACCEPT, "*/*")
                .header(REFERER, format!("{BASE_URL}/u/{id}"));
            if offset > 0 {
                request = request.header(RANGE, format!("bytes={offset}-"));
            }

            let Ok(response) = request.send().await else {
                continue;
            };
            if response.status().is_success() && !looks_like_challenge(&response, file_name) {
                return Some(response);
            }
        }
        None
    }

    async fn official_response(&self, id: &str, offset: u64) -> Result<Response, UpioError> {
        let mut request = self
            .client
            .get(format!("{FILE_API}/{id}?download"))
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "*/*");
        if let Some(auth) = self.auth() {
            request = request.header(AUTHORIZATION, auth);
        }
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }
        request.send().await.map_err(map_reqwest_error)
    }
}

impl Default for PixeldrainDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Downloader for PixeldrainDownloader {
    fn client(&self) -> &Client {
        &self.client
    }

    async fn resolve(
        &self,
        url: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError> {
        let parsed = Url::parse(url).map_err(|error| UpioError {
            message: format!("invalid Pixeldrain URL '{url}': {error}"),
            status_code: None,
        })?;

        match resource(&parsed) {
            Some((ResourceKind::File, id)) => {
                let info = self.info(id).await?;
                let name = if info.name.trim().is_empty() {
                    id.to_string()
                } else {
                    info.name
                };
                let mut file = RemoteFile::new(name, format!("{BASE_URL}/u/{id}"));
                file.size = info.size;
                Ok(RemoteNode::File(file))
            }
            Some((ResourceKind::Collection, id)) => {
                let list = self.list(id).await?;
                let name = list
                    .title
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or_else(|| format!("pixeldrain-{id}"));
                let children = list
                    .files
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|file| file.name != ".search_index.gz")
                    .map(|file| {
                        let mut remote = RemoteFile::new(
                            if file.name.trim().is_empty() {
                                file.id.clone()
                            } else {
                                file.name
                            },
                            format!("{BASE_URL}/u/{}", file.id),
                        );
                        remote.size = file.size;
                        RemoteNode::File(remote)
                    })
                    .collect::<Vec<_>>();

                if children.is_empty() {
                    return Err(UpioError {
                        message: format!("Pixeldrain list '{id}' contained no files"),
                        status_code: None,
                    });
                }
                Ok(RemoteNode::Folder { name, children })
            }
            None => Err(UpioError {
                message: format!("'{url}' is not a Pixeldrain file or list URL"),
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
        let parsed = Url::parse(&file.url).map_err(|error| UpioError {
            message: format!("invalid Pixeldrain file URL '{}': {error}", file.url),
            status_code: None,
        })?;
        let (_, id) = resource(&parsed).ok_or_else(|| UpioError {
            message: format!("Pixeldrain file URL carries no id: {}", file.url),
            status_code: None,
        })?;

        let proxy_first = self.api_key.is_none() && offset == 0;
        if proxy_first {
            if let Some(response) = self.proxy_response(id, offset, &file.name).await {
                return Ok(response);
            }
        }

        match self.official_response(id, offset).await {
            Ok(response) if response.status().is_success() => Ok(response),
            Ok(response) => {
                if let Some(proxy) = self.proxy_response(id, offset, &file.name).await {
                    Ok(proxy)
                } else {
                    Ok(response)
                }
            }
            Err(error) => {
                if let Some(proxy) = self.proxy_response(id, offset, &file.name).await {
                    Ok(proxy)
                } else {
                    Err(error)
                }
            }
        }
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

pub fn parse_url(url: &str) -> Option<ParsedUrl> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let known = matches!(
        host,
        "pixeldrain.com"
            | "www.pixeldrain.com"
            | "pixeldrain.net"
            | "pixeldrain.dev"
            | "pixeldra.in"
    );
    if !known {
        return None;
    }

    resource(&parsed).map(|(kind, id)| ParsedUrl {
        id: id.to_string(),
        kind,
    })
}

fn resource(url: &Url) -> Option<(ResourceKind, &str)> {
    let mut segments = url.path_segments()?;
    let kind = match segments.next()? {
        "u" | "file" => ResourceKind::File,
        "l" => ResourceKind::Collection,
        _ => return None,
    };
    let id = segments.next().filter(|id| !id.is_empty())?;
    Some((kind, id))
}

fn normalize_proxy(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let value = if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };
    Some(if value.ends_with('/') {
        value
    } else {
        format!("{value}/")
    })
}

fn looks_like_challenge(response: &Response, file_name: &str) -> bool {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let name = file_name.to_ascii_lowercase();
    if content_type.starts_with("text/html") && !name.ends_with(".html") && !name.ends_with(".htm")
    {
        return true;
    }
    if content_type.starts_with("application/json") && !name.ends_with(".json") {
        return true;
    }
    false
}

fn api_error(value: &Value, status_code: Option<u16>) -> UpioError {
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| value.get("value").and_then(Value::as_str))
        .unwrap_or("Pixeldrain rejected the request");
    UpioError {
        message: message.to_string(),
        status_code,
    }
}

pub static PIXELDRAIN_DOWNLOAD: DownloadDef = DownloadDef {
    parse: parse_url,
    credential: upio_core::Credential::OptionalToken,
    build,
};

fn build(
    config: &UploaderEndpointConfig,
) -> Result<upio_core::BoxedDownloader, upio_core::BuildError> {
    Ok(Box::new(match config.token.as_deref() {
        Some(token) if !token.trim().is_empty() => PixeldrainDownloader::with_token(token),
        _ => PixeldrainDownloader::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_urls_are_detected() {
        assert_eq!(
            parse_url("https://pixeldrain.com/u/abc123").unwrap().kind,
            ResourceKind::File
        );
        assert_eq!(
            parse_url("https://pixeldra.in/l/abc123").unwrap().kind,
            ResourceKind::Collection
        );
        assert_eq!(
            parse_url("https://www.pixeldrain.com/u/abc123").unwrap().id,
            "abc123"
        );
        assert!(parse_url("https://pixeldrain.com/api/file/abc123").is_none());
        assert!(parse_url("https://pixeldrain.com/d/abc123").is_none());
        assert!(parse_url("https://example.com/u/abc123").is_none());
    }

    #[test]
    fn proxy_entries_are_normalized() {
        assert_eq!(
            normalize_proxy("cdn.example").as_deref(),
            Some("https://cdn.example/")
        );
        assert_eq!(
            normalize_proxy("https://cdn.example/path").as_deref(),
            Some("https://cdn.example/path/")
        );
        assert_eq!(normalize_proxy("  "), None);
    }
}
