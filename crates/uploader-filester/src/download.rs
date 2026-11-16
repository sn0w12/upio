//! Filester download resolution.
//!
//! A share page is a thin shell around a public download-token endpoint. The
//! token is short-lived, so a fresh one is minted on every transfer attempt and
//! turned into a signed CDN URL.

use async_trait::async_trait;
use reqwest::header::{ACCEPT, CONTENT_TYPE, ORIGIN, RANGE, REFERER, USER_AGENT};
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

pub struct FilesterDownloader {
    client: Client,
}

impl FilesterDownloader {
    pub const NAME: &str = "filester";

    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    async fn token(&self, slug: &str, page: &Url) -> Result<DownloadToken, UpioError> {
        let response = self
            .client
            .post(TOKEN_URL)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/json, text/plain, */*")
            .header(CONTENT_TYPE, "application/json")
            .header(ORIGIN, PAGE_ORIGIN)
            .header(REFERER, page.as_str())
            .json(&serde_json::json!({ "file_slug": slug }))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        let token: DownloadToken = response.json().await.map_err(map_reqwest_error)?;
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
        _config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError> {
        let page = Url::parse(url).map_err(|error| UpioError {
            message: format!("invalid Filester URL '{url}': {error}"),
            status_code: None,
        })?;
        let slug = slug(&page).ok_or_else(|| UpioError {
            message: format!("'{url}' is not a Filester file URL"),
            status_code: None,
        })?;

        let token = self.token(slug, &page).await?;
        let name = token
            .name
            .filter(|name| !name.trim().is_empty())
            .or_else(|| token.file.clone())
            .unwrap_or_else(|| slug.to_string());

        Ok(RemoteNode::File(RemoteFile::new(name, page.to_string())))
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
/// Filester's public share shape is `/d/<slug>`. Account and CDN hosts are not
/// share URLs.
pub fn parse_url(url: &str) -> Option<ParsedUrl> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if host != "filester.me" && host != "www.filester.me" {
        return None;
    }
    let slug = slug(&parsed)?;
    Some(ParsedUrl {
        id: slug.to_string(),
        kind: ResourceKind::File,
    })
}

fn slug(url: &Url) -> Option<&str> {
    let mut segments = url.path_segments()?;
    if segments.next()? != "d" {
        return None;
    }
    segments.next().filter(|slug| !slug.is_empty())
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
    credential: upio_core::Credential::None,
    build,
};

fn build(
    _config: &UploaderEndpointConfig,
) -> Result<upio_core::BoxedDownloader, upio_core::BuildError> {
    Ok(Box::new(FilesterDownloader::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn share_urls_are_detected() {
        assert_eq!(
            parse_url("https://filester.me/d/abc123").unwrap().id,
            "abc123"
        );
        assert_eq!(
            parse_url("https://www.filester.me/d/abc123").unwrap().id,
            "abc123"
        );
        assert!(parse_url("https://filester.me/api/v1/upload").is_none());
        assert!(parse_url("https://filester.me/d/").is_none());
        assert!(parse_url("https://example.com/d/abc123").is_none());
        assert!(parse_url("https://u1.filester.me/d/abc123").is_none());
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
