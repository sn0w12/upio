//! Downloader for plain HTTP(S) URLs.
//!
//! This is the fallback for links that no service claims, and the way direct
//! CDN links produced by an upload are fetched.

use async_trait::async_trait;

use crate::download::{Downloader, RemoteFile, RemoteNode};
use crate::error::UpioError;
use crate::types::UploaderEndpointConfig;

/// Fetch any `http(s)` URL without service-specific resolution.
pub struct DirectDownloader {
    client: reqwest::Client,
}

impl DirectDownloader {
    pub const NAME: &str = "direct";

    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for DirectDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Downloader for DirectDownloader {
    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn resolve(
        &self,
        url: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<RemoteNode, UpioError> {
        let parsed = reqwest::Url::parse(url).map_err(|error| UpioError {
            message: format!("invalid URL '{url}': {error}"),
            status_code: None,
        })?;

        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(UpioError {
                message: format!(
                    "'{url}' uses the unsupported scheme '{}'; only http and https work",
                    parsed.scheme()
                ),
                status_code: None,
            });
        }

        // HEAD is an optimization for the display name and size. A server that
        // refuses it still downloads through `open`, so failure is not fatal.
        let (encoded_name, size) = match self.client.head(parsed.clone()).send().await {
            Ok(response) if response.status().is_success() => (
                response
                    .headers()
                    .get(reqwest::header::CONTENT_DISPOSITION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(content_disposition_filename),
                content_length(&response),
            ),
            _ => (None, None),
        };

        let name = encoded_name
            .or_else(|| url_file_name(&parsed))
            .unwrap_or_else(|| "download".to_string());

        let mut file = RemoteFile::new(name, url);
        file.size = size;
        Ok(RemoteNode::File(file))
    }

    fn name(&self) -> &str {
        Self::NAME
    }
}

fn url_file_name(url: &reqwest::Url) -> Option<String> {
    let segment = url
        .path_segments()?
        .rev()
        .find(|segment| !segment.is_empty())?;
    let decoded = percent_decode(segment);
    let decoded = decoded.trim();
    (!decoded.is_empty()).then(|| decoded.to_string())
}

fn content_length(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
}

fn content_disposition_filename(header: &str) -> Option<String> {
    let mut plain = None;
    let mut extended = None;

    for parameter in split_parameters(header) {
        let Some((key, value)) = parameter.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"');
        match key.as_str() {
            "filename*" => extended = decode_extended(value),
            "filename" => plain = Some(value.to_string()),
            _ => {}
        }
    }

    extended
        .filter(|name| !name.trim().is_empty())
        .or_else(|| plain.filter(|name| !name.trim().is_empty()))
}

fn split_parameters(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (index, ch) in value.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                parts.push(value[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim());
    parts
}

/// Decode an RFC 5987 extended value such as `UTF-8''my%20file.txt`.
fn decode_extended(value: &str) -> Option<String> {
    let (_, encoded) = value.split_once("''")?;
    Some(percent_decode(encoded))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                decoded.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_come_from_the_last_path_segment() {
        let url = reqwest::Url::parse("https://example.test/a/b/photo%20one.png?x=1").unwrap();
        assert_eq!(url_file_name(&url).as_deref(), Some("photo one.png"));
    }

    #[test]
    fn a_url_without_a_path_has_no_name() {
        let url = reqwest::Url::parse("https://example.test/").unwrap();
        assert_eq!(url_file_name(&url), None);
    }

    #[test]
    fn quoted_content_disposition_names_are_used() {
        assert_eq!(
            content_disposition_filename("attachment; filename=\"my file.mp4\"").as_deref(),
            Some("my file.mp4")
        );
    }

    #[test]
    fn extended_content_disposition_names_win() {
        assert_eq!(
            content_disposition_filename(
                "attachment; filename=\"fallback.mp4\"; filename*=UTF-8''real%20name.mp4"
            )
            .as_deref(),
            Some("real name.mp4")
        );
    }

    #[test]
    fn semicolons_inside_quotes_are_not_parameter_separators() {
        assert_eq!(
            content_disposition_filename("attachment; filename=\"a;b.txt\"").as_deref(),
            Some("a;b.txt")
        );
    }

    #[test]
    fn empty_disposition_names_are_ignored() {
        assert_eq!(
            content_disposition_filename("attachment; filename=\"\""),
            None
        );
    }
}
