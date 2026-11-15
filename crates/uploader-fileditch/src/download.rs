//! Fileditch download resolution.
//!
//! Share pages sit behind a WebAssembly proof-of-work check. The check is
//! solved by the page's own script, so this resolver lets a real Chrome run it
//! and then reads the direct link the page assigns to its download button.
//! The returned URL is public and resumable, so the shared engine handles the
//! actual transfer.

use std::time::{Duration, Instant};

use async_trait::async_trait;

use upio_browser::BrowserSession;
use upio_core::download::{Downloader, ParsedUrl, RemoteFile, RemoteNode, ResourceKind};
use upio_core::error::UpioError;
use upio_core::service::DownloadDef;
use upio_core::types::UploaderEndpointConfig;

const RESOLVE_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Reads the direct link the player script assigns to the download button.
const READ_DOWNLOAD_HREF: &str = r#"
(() => {
  const button = document.querySelector('a.btn-main[download]');
  const video = document.querySelector('video');
  const source = document.querySelector('source');
  const href = (button && button.href)
    || (video && video.currentSrc)
    || (source && source.getAttribute('src') ? source.src : '')
    || null;
  if (!href) return null;
  const path = document.querySelector('.path');
  const name = (document.title || '').trim()
    || (path && path.textContent ? path.textContent.trim() : '');
  return { href, name };
})()
"#;

struct ResolvedLink {
    href: String,
    name: Option<String>,
}

pub struct FileditchDownloader {
    client: reqwest::Client,
}

impl FileditchDownloader {
    pub const NAME: &str = "fileditch";

    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for FileditchDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Downloader for FileditchDownloader {
    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn resolve(
        &self,
        url: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError> {
        let link = resolve_with_browser(url).await?;
        let name = link
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback_name(url));
        Ok(RemoteNode::File(RemoteFile::new(name, link.href)))
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

/// Classify a Fileditch share URL.
///
/// Only the canonical share shape (`/<user>/<id>/<filename>`) is claimed. The
/// signed media hosts use a single token segment, so they fall through to the
/// generic direct downloader.
pub fn parse_url(url: &str) -> Option<ParsedUrl> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let known = matches!(
        host.as_str(),
        "fileditch.com" | "www.fileditch.com" | "fileditchvids.st" | "fileditchfiles.st"
    );
    if !known {
        return None;
    }

    let segments = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .count();
    (segments >= 3).then(|| ParsedUrl {
        id: parsed.path().trim_start_matches('/').to_string(),
        kind: ResourceKind::File,
    })
}

async fn resolve_with_browser(url: &str) -> Result<ResolvedLink, UpioError> {
    let session = BrowserSession::launch().await.map_err(browser_error)?;
    session.goto(url).await.map_err(browser_error)?;

    let deadline = Instant::now() + RESOLVE_TIMEOUT;
    loop {
        if let Some(link) = read_download_href(&session).await {
            return Ok(link);
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    Err(UpioError {
        message: format!(
            "fileditch did not expose a download link within {}s; Chrome needs a display on a \
             headless Linux host, or open the page once in the profile at {}",
            RESOLVE_TIMEOUT.as_secs(),
            session.profile().display()
        ),
        status_code: None,
    })
}

async fn read_download_href(session: &BrowserSession) -> Option<ResolvedLink> {
    let value = session.evaluate(READ_DOWNLOAD_HREF).await.ok().flatten()?;
    let href = value.get("href")?.as_str()?.trim().to_string();
    if href.is_empty() {
        return None;
    }
    let name = value
        .get("name")
        .and_then(|name| name.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);
    Some(ResolvedLink { href, name })
}

fn fallback_name(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
        })
        .filter(|segment| !segment.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

fn browser_error(error: impl std::fmt::Display) -> UpioError {
    UpioError {
        message: format!(
            "fileditch downloads need Chrome to pass the page's browser check: {error}"
        ),
        status_code: None,
    }
}

pub static FILEDITCH_DOWNLOAD: DownloadDef = DownloadDef {
    parse: parse_url,
    credential: upio_core::Credential::None,
    build,
};

fn build(
    _config: &UploaderEndpointConfig,
) -> Result<upio_core::BoxedDownloader, upio_core::BuildError> {
    Ok(Box::new(FileditchDownloader::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_urls_are_claimed() {
        let parsed = parse_url("https://fileditchvids.st/alice/abc123/file.mp4").unwrap();
        assert_eq!(parsed.kind, ResourceKind::File);
        assert_eq!(parsed.id, "alice/abc123/file.mp4");
    }

    #[test]
    fn signed_media_and_unknown_hosts_are_not_claimed() {
        assert!(parse_url("https://charlie.fileditchvids.st/MTc1MjM0").is_none());
        assert!(parse_url("https://fileditchvids.st/MTc1MjM0").is_none());
        assert!(parse_url("https://example.test/alice/abc123/file.mp4").is_none());
    }

    #[test]
    fn fallback_names_come_from_the_path() {
        assert_eq!(
            fallback_name("https://fileditchvids.st/alice/abc123/file.mp4"),
            "file.mp4"
        );
        assert_eq!(fallback_name("https://fileditchvids.st/"), "download");
    }
}
