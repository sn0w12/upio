//! Bunkr download resolution.
//!
//! Bunkr exposes a numeric file id on each share page. The public download API
//! turns that id into a media path, a separate signer issues a short-lived
//! token, and the resulting URL is a plain range-capable CDN link. The link is
//! minted per request so retries after an expiry start clean. An optional
//! account token travels with page and API requests; public links work without
//! one.

use async_trait::async_trait;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, ORIGIN, RANGE, REFERER, USER_AGENT};
use reqwest::{Client, Response, Url};
use serde::Deserialize;

use upio_core::download::{Downloader, ParsedUrl, RemoteFile, RemoteNode, ResourceKind};
use upio_core::error::UpioError;
use upio_core::http::{file_name_from_path, map_reqwest_error};
use upio_core::service::DownloadDef;
use upio_core::types::UploaderEndpointConfig;

const API_URL: &str = "https://dl.bunkr.cr/api/_001_v2";
const SIGN_URL: &str = "https://glb-apisign.cdn.cr/sign";
const API_ORIGIN: &str = "https://bunkr.cr";
const CDN_REFERER: &str = "https://dl.bunkrr.cr/";

const PAGE_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                               AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 \
                               Safari/537.36";
const CDN_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:151.0) \
                              Gecko/20100101 Firefox/151.0";

/// A private query parameter carrying the numeric file id on a page URL.
const ID_PARAM: &str = "upio-id";

#[derive(Debug, Deserialize)]
struct AlbumFile {
    id: i64,
    #[serde(default)]
    original: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    size: i64,
}

#[derive(Debug, Deserialize)]
struct DownloadResponse {
    mediafiles: String,
    path: String,
    #[serde(default)]
    original: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SignResponse {
    token: String,
    ex: i64,
}

pub struct BunkrDownloader {
    client: Client,
    token: Option<String>,
}

impl BunkrDownloader {
    pub const NAME: &str = "bunkr";

    pub fn new() -> Self {
        Self {
            client: Client::new(),
            token: None,
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self {
            client: Client::new(),
            token: Some(token.to_string()),
        }
    }
}

impl Default for BunkrDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Downloader for BunkrDownloader {
    fn client(&self) -> &Client {
        &self.client
    }

    async fn resolve(
        &self,
        url: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError> {
        resolve_page(&self.client, self.token.as_deref(), url).await
    }

    async fn open(
        &self,
        file: &RemoteFile,
        offset: u64,
        _config: &UploaderEndpointConfig,
    ) -> Result<Response, UpioError> {
        let id = file_id(&file.url).ok_or_else(|| UpioError {
            message: format!("bunkr file URL carries no numeric file id: {}", file.url),
            status_code: None,
        })?;
        let direct = signed_url(&self.client, self.token.as_deref(), &id).await?;

        let mut request = self
            .client
            .get(direct)
            .header(USER_AGENT, CDN_USER_AGENT)
            .header(ACCEPT, "*/*")
            .header(ACCEPT_LANGUAGE, "en-US,en;q=0.5")
            .header(REFERER, CDN_REFERER);
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }

        request.send().await.map_err(map_reqwest_error)
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

/// Classify a Bunkr share URL.
///
/// Bunkr rotates domains, so any two-label `bunkr*` host is accepted. Only
/// `/f/<id>` and `/a/<id>` paths are share URLs; API and asset hosts are
/// excluded because they add a subdomain.
pub fn parse_url(url: &str) -> Option<ParsedUrl> {
    let parsed = Url::parse(url).ok()?;
    if !is_bunkr_host(parsed.host_str()?) {
        return None;
    }
    let kind = classify(&parsed)?;
    Some(ParsedUrl {
        id: parsed.path().trim_start_matches('/').to_string(),
        kind,
    })
}

fn is_bunkr_host(host: &str) -> bool {
    match host.split('.').collect::<Vec<_>>().as_slice() {
        [name, _] => name.starts_with("bunkr"),
        ["www", name, _] => name.starts_with("bunkr"),
        _ => false,
    }
}

fn classify(url: &Url) -> Option<ResourceKind> {
    let mut segments = url.path_segments()?;
    let first = segments.next()?;
    let id = segments.next()?;
    if id.is_empty() {
        return None;
    }
    match first {
        "f" => Some(ResourceKind::File),
        "a" => Some(ResourceKind::Collection),
        _ => None,
    }
}

async fn resolve_page(
    client: &Client,
    token: Option<&str>,
    url: &str,
) -> Result<RemoteNode, UpioError> {
    let parsed = Url::parse(url).map_err(|error| UpioError {
        message: format!("invalid Bunkr URL '{url}': {error}"),
        status_code: None,
    })?;
    match classify(&parsed) {
        Some(ResourceKind::File) => resolve_file(client, token, parsed).await,
        Some(ResourceKind::Collection) => resolve_album(client, token, parsed).await,
        None => Err(UpioError {
            message: format!("'{url}' is not a Bunkr file or album URL"),
            status_code: None,
        }),
    }
}

async fn resolve_file(
    client: &Client,
    token: Option<&str>,
    url: Url,
) -> Result<RemoteNode, UpioError> {
    let (final_url, html) = get_page(client, token, url).await?;
    let id = extract_file_id(&html).ok_or_else(|| UpioError {
        message: "the Bunkr file page did not contain a numeric file id".to_string(),
        status_code: None,
    })?;

    let name = meta_content(&html, "og:title")
        .or_else(|| {
            h1_text(
                &html,
                "text-subs font-semibold text-base sm:text-lg truncate",
            )
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| file_name_from_path(final_url.path()));

    let mut page_url = final_url;
    page_url.query_pairs_mut().append_pair(ID_PARAM, &id);

    let mut file = RemoteFile::new(name, page_url.to_string());
    file.size = debug_size(&html);
    Ok(RemoteNode::File(file))
}

async fn resolve_album(
    client: &Client,
    token: Option<&str>,
    mut url: Url,
) -> Result<RemoteNode, UpioError> {
    url.query_pairs_mut().append_pair("advanced", "1");
    let (final_url, html) = get_page(client, token, url).await?;

    let array = extract_album_array(&html).ok_or_else(|| UpioError {
        message: "the Bunkr album page did not contain window.albumFiles".to_string(),
        status_code: None,
    })?;
    let files: Vec<AlbumFile> = json5::from_str(array).map_err(|error| UpioError {
        message: format!("unreadable Bunkr album data: {error}"),
        status_code: None,
    })?;

    let name = meta_content(&html, "og:title")
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "bunkr-album".to_string());

    let origin = origin_of(&final_url);
    let children = files
        .into_iter()
        .filter_map(|file| album_child(&origin, file).map(RemoteNode::File))
        .collect::<Vec<_>>();

    if children.is_empty() {
        return Err(UpioError {
            message: "the Bunkr album contained no downloadable files".to_string(),
            status_code: None,
        });
    }

    Ok(RemoteNode::Folder { name, children })
}

fn album_child(origin: &str, file: AlbumFile) -> Option<RemoteFile> {
    if file.id <= 0 {
        return None;
    }
    let name = if file.original.trim().is_empty() {
        file.name
    } else {
        file.original
    };
    if name.trim().is_empty() {
        return None;
    }
    let slug = if file.slug.trim().is_empty() {
        file.id.to_string()
    } else {
        file.slug
    };
    let mut page_url = Url::parse(&format!("{origin}/f/{slug}")).ok()?;
    page_url
        .query_pairs_mut()
        .append_pair(ID_PARAM, &file.id.to_string());

    let mut remote = RemoteFile::new(name, page_url.to_string());
    remote.size = u64::try_from(file.size).ok().filter(|size| *size > 0);
    Some(remote)
}

fn origin_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or("bunkr.cr");
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

async fn get_page(
    client: &Client,
    token: Option<&str>,
    url: Url,
) -> Result<(Url, String), UpioError> {
    let mut request = client
        .get(url)
        .header(USER_AGENT, PAGE_USER_AGENT)
        .header(
            ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9");
    if let Some(token) = token {
        request = request.header("token", token);
    }
    let response = request.send().await.map_err(map_reqwest_error)?;

    let status = response.status();
    if !status.is_success() {
        return Err(UpioError {
            message: format!("the Bunkr page returned {status}"),
            status_code: Some(status.as_u16()),
        });
    }

    let final_url = response.url().clone();
    let html = response.text().await.map_err(map_reqwest_error)?;
    Ok((final_url, html))
}

async fn signed_url(client: &Client, token: Option<&str>, id: &str) -> Result<Url, UpioError> {
    let mut request = client
        .post(API_URL)
        .header(USER_AGENT, PAGE_USER_AGENT)
        .header(ACCEPT, "application/json, text/plain, */*")
        .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .header(CONTENT_TYPE, "application/json")
        .header(ORIGIN, API_ORIGIN);
    if let Some(token) = token {
        request = request.header("token", token);
    }
    let meta: DownloadResponse = request
        .json(&serde_json::json!({ "id": id }))
        .send()
        .await
        .map_err(map_reqwest_error)?
        .error_for_status()
        .map_err(map_reqwest_error)?
        .json::<DownloadResponse>()
        .await
        .map_err(map_reqwest_error)?;

    let sign: SignResponse = client
        .get(SIGN_URL)
        .query(&[("path", meta.path.as_str())])
        .header(USER_AGENT, PAGE_USER_AGENT)
        .header(ACCEPT, "application/json, text/plain, */*")
        .send()
        .await
        .map_err(map_reqwest_error)?
        .error_for_status()
        .map_err(map_reqwest_error)?
        .json::<SignResponse>()
        .await
        .map_err(map_reqwest_error)?;

    build_direct_url(&meta, &sign)
}

fn build_direct_url(meta: &DownloadResponse, sign: &SignResponse) -> Result<Url, UpioError> {
    let raw = format!("{}{}", meta.mediafiles, meta.path);
    let mut url = Url::parse(&raw).map_err(|error| UpioError {
        message: format!("Bunkr returned an invalid media URL: {error}"),
        status_code: None,
    })?;

    {
        let mut query = url.query_pairs_mut();
        if let Some(original) = meta
            .original
            .as_deref()
            .filter(|original| !original.trim().is_empty())
        {
            query.append_pair("n", original);
        }
        query.append_pair("token", &sign.token);
        query.append_pair("ex", &sign.ex.to_string());
    }

    Ok(url)
}

fn file_id(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if let Some((_, value)) = parsed.query_pairs().find(|(name, _)| name == ID_PARAM) {
        if !value.is_empty() {
            return Some(value.into_owned());
        }
    }

    let segment = parsed.path_segments()?.next_back()?;
    segment
        .chars()
        .all(|ch| ch.is_ascii_digit())
        .then(|| segment.to_string())
}

fn extract_file_id(html: &str) -> Option<String> {
    let marker = "data-file-id=\"";
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find('"')? + start;
    let id = html[start..end].trim();
    (!id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit())).then(|| id.to_string())
}

fn extract_album_array(html: &str) -> Option<&str> {
    let marker = "window.albumFiles";
    let marker_start = html.find(marker)?;
    let start = html[marker_start..].find('[')? + marker_start;

    let mut depth = 0usize;
    for (offset, ch) in html[start..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(&html[start..=start + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

fn meta_content(html: &str, property: &str) -> Option<String> {
    let marker = format!("property=\"{property}\"");
    let index = html.find(&marker)?;
    let rest = &html[index + marker.len()..];
    let content = rest.find("content=\"")? + "content=\"".len();
    let end = rest[content..].find('"')? + content;
    Some(decode_entities(&rest[content..end]))
}

fn h1_text(html: &str, class: &str) -> Option<String> {
    let marker = format!("class=\"{class}\"");
    let start = html.find(&marker)?;
    let open = html[start..].find('>')? + start;
    let close = html[open + 1..].find("</h1>")? + open + 1;
    Some(decode_entities(html[open + 1..close].trim()))
}

fn debug_size(html: &str) -> Option<u64> {
    let debug = html.find("Debug:")?;
    let start = html[debug..].find("Size=")? + debug + "Size=".len();
    let digits = html[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
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

pub static BUNKR_DOWNLOAD: DownloadDef = DownloadDef {
    parse: parse_url,
    credential: upio_core::Credential::OptionalToken,
    build,
};

fn build(
    config: &UploaderEndpointConfig,
) -> Result<upio_core::BoxedDownloader, upio_core::BuildError> {
    Ok(Box::new(match config.token.as_deref() {
        Some(token) if !token.trim().is_empty() => BunkrDownloader::with_token(token),
        _ => BunkrDownloader::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn share_urls_are_classified() {
        let file = parse_url("https://bunkr.example/f/abc123xy").unwrap();
        assert_eq!(file.kind, ResourceKind::File);
        assert_eq!(file.id, "f/abc123xy");

        let album = parse_url("https://bunkr.example/a/album42").unwrap();
        assert_eq!(album.kind, ResourceKind::Collection);
    }

    #[test]
    fn non_share_hosts_and_paths_are_rejected() {
        assert!(parse_url("https://bunkr.example/css/master.css").is_none());
        assert!(parse_url("https://dl.bunkr.example/file/12345678").is_none());
        assert!(parse_url("https://dash.bunkr.example/api/node").is_none());
        assert!(parse_url("https://albums.example/a/album42").is_none());
        assert!(parse_url("https://example.com/f/abc123xy").is_none());
    }

    #[test]
    fn the_numeric_file_id_is_read_from_the_page() {
        let html = r#"<script defer data-file-id="12345678" src="../js/lv.js"></script>"#;
        assert_eq!(extract_file_id(html).as_deref(), Some("12345678"));
        assert_eq!(
            debug_size("Debug: Original=a.mp4, Size=135173018"),
            Some(135173018)
        );
    }

    #[test]
    fn the_album_array_parses_js_object_notation() {
        let html = r#"
            <script>
            window.albumFiles = [
              { id: 1, name: "a.mp4", original: "a.mp4", slug: "slugA", size: 10, type: "video/mp4", },
              { id: 2, original: "b.mp4", slug: "slugB", size: 20, },
            ];
            </script>
        "#;
        let array = extract_album_array(html).expect("array");
        let files: Vec<AlbumFile> = json5::from_str(array).expect("parse");

        let children = files
            .into_iter()
            .filter_map(|file| album_child("https://bunkr.example", file))
            .collect::<Vec<_>>();

        assert_eq!(children.len(), 2);
        assert_eq!(children[0].name, "a.mp4");
        assert_eq!(children[0].size, Some(10));
        assert!(children[0].url.contains("f/slugA"));
        assert!(children[0].url.contains("upio-id=1"));
    }

    #[test]
    fn the_file_id_is_read_back_from_the_page_url() {
        assert_eq!(
            file_id("https://bunkr.example/f/slugA?upio-id=123").as_deref(),
            Some("123")
        );
        assert_eq!(
            file_id("https://bunkr.example/f/123").as_deref(),
            Some("123")
        );
        assert_eq!(file_id("https://bunkr.example/f/slugA"), None);
    }

    #[test]
    fn the_direct_url_carries_the_original_name_and_token() {
        let meta = DownloadResponse {
            mediafiles: "https://cdn.example".to_string(),
            path: "/storage/media/a.mp4".to_string(),
            original: Some("a.mp4".to_string()),
        };
        let sign = SignResponse {
            token: "token".to_string(),
            ex: 42,
        };

        let url = build_direct_url(&meta, &sign).unwrap();
        assert_eq!(url.host_str(), Some("cdn.example"));
        assert_eq!(url.path(), "/storage/media/a.mp4");

        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query.get("n").map(String::as_str), Some("a.mp4"));
        assert_eq!(query.get("token").map(String::as_str), Some("token"));
        assert_eq!(query.get("ex").map(String::as_str), Some("42"));
    }

    #[test]
    fn html_entities_are_decoded() {
        assert_eq!(decode_entities("a &amp; b"), "a & b");
    }
}
