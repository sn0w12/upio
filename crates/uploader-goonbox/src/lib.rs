//! GoonBox (goonbox.cr) uploader.
//!
//! GoonBox is an image host. Its documented `/api/v1` API is read-only and
//! answers `upload_disabled` on `POST /api/v1/images`, so the only write route
//! is `POST /api/upload`, reached with the bearer token that
//! `POST /api/auth/login` returns. That login is gated by a Cloudflare
//! Turnstile token verified before credentials, so an upload needs a captcha
//! token as well as a username and password. See [`login`] for the auth chain
//! and [`waf`] for how a captcha token is obtained without a human.

mod login;
mod waf;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use upio_core::error::UploadError;
use upio_core::filesize::FileSize;
use upio_core::http::{map_reqwest_error, tracked_file_part};
use upio_core::types::{UploadResult, Uploader, UploaderCapabilities, UploaderEndpointConfig};

use login::{Credentials, Session, XSRF_HEADER};

/// `reqwest` multipart forms are consumed by `RequestBuilder`, so the 401 retry
/// has to rebuild the body rather than hand the same one over twice.
fn image_form(
    file_path: &str,
    album_id: Option<&str>,
    progress: Option<&upio_core::http::UploadProgress>,
) -> impl std::future::Future<Output = Result<reqwest::multipart::Form, UploadError>> + Send {
    let album_id = album_id.map(str::to_string);
    let progress = progress.cloned();
    let path = file_path.to_string();
    async move {
        let part = tracked_file_part(&path, progress.as_ref()).await?;
        let mut form = reqwest::multipart::Form::new().part("file", part);
        if let Some(album_id) = album_id {
            form = form.text("album_id", album_id);
        }
        Ok(form)
    }
}

const UPLOAD_URL: &str = "https://goonbox.cr/api/upload";
const ALBUMS_URL: &str = "https://goonbox.cr/api/albums";

/// GoonBox paginates albums and rejects `per_page` above 500, so name lookups
/// have to page rather than ask for everything at once.
const ALBUMS_PER_PAGE: &str = "500";

#[derive(Deserialize)]
struct UploadImage {
    thumb_url: Option<String>,
}

#[derive(Deserialize)]
struct UploadResponse {
    image: Option<UploadImage>,
}

#[derive(Deserialize, Clone)]
struct Album {
    title: String,
    encoded_id: String,
}

#[derive(Deserialize)]
struct AlbumsResponse {
    albums: Vec<Album>,
}

/// GoonBox uploader.
///
/// Construction is synchronous; everything network-bound happens in
/// [`Uploader::init`]. The captcha token is a cache: when it is missing or
/// rejected, a fresh one is solved with a real Chrome and reused for the rest
/// of the run.
pub struct GoonboxUploader {
    session: Arc<Session>,
    /// A bearer rejected as expired is discarded once and replaced, so the
    /// next attempt in the same batch does not log in again.
    rotated: RwLock<bool>,
}

impl GoonboxUploader {
    pub const CAPABILITIES: UploaderCapabilities = UploaderCapabilities {
        image: true,
        ..UploaderCapabilities::none()
    };

    /// `GET /api/upload/config` reports `max_bytes` of 25 MiB.
    pub const MAX_FILE_SIZE: FileSize = FileSize::from_mb(25);

    pub fn new(username: &str, password: &str) -> Self {
        Self::with_profile(username, password, default_profile_dir())
    }

    pub fn with_profile(username: &str, password: &str, profile_dir: PathBuf) -> Self {
        let credentials = Credentials {
            username: username.to_string(),
            password: password.to_string(),
            solved_captcha: RwLock::new(None),
            profile_dir,
        };

        let session = Session::new(credentials).unwrap_or_else(|_| Session::new_fallback());

        Self {
            session: Arc::new(session),
            rotated: RwLock::new(false),
        }
    }

    /// The bearer, re-authenticating once if the cached one is rejected.
    async fn authorized_get(&self, url: &str) -> Result<reqwest::Response, UploadError> {
        let response = self.send_with_bearer(url).await?;

        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        if *self.rotated.read().await {
            return Ok(response);
        }

        *self.rotated.write().await = true;
        self.session.invalidate().await;
        self.send_with_bearer(url).await
    }

    async fn send_with_bearer(&self, url: &str) -> Result<reqwest::Response, UploadError> {
        let bearer = self.session.bearer().await?;
        self.session
            .client()
            .get(url)
            .header("Authorization", format!("Bearer {bearer}"))
            .send()
            .await
            .map_err(map_reqwest_error)
    }

    /// Log in, solving a captcha first if none is cached or the last one was
    /// refused. GoonBox checks the captcha before the password, so a stale
    /// token looks exactly like a wrong password.
    async fn ensure_session(&self) -> Result<(), UploadError> {
        if self.session.bearer().await.is_ok() {
            return Ok(());
        }

        // GoonBox verifies the challenge before the password, so a login with no
        // token can only fail and would misreport the cause as bad credentials.
        let solved = self.session.refresh_captcha().await?;
        self.session.set_solved_captcha(solved).await;
        self.session.bearer().await?;
        Ok(())
    }
}

impl Default for GoonboxUploader {
    fn default() -> Self {
        Self::new("", "")
    }
}

fn default_profile_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("upio")
        .join("chrome-profile")
}

#[async_trait]
impl Uploader for GoonboxUploader {
    async fn init(&self) -> Result<(), UploadError> {
        self.ensure_session().await
    }

    async fn upload_file(
        &self,
        file_path: &str,
        config: &UploaderEndpointConfig,
    ) -> Result<UploadResult, UploadError> {
        self.upload_file_with_progress(file_path, config, None)
            .await
    }

    async fn upload_file_with_progress(
        &self,
        file_path: &str,
        config: &UploaderEndpointConfig,
        progress: Option<upio_core::http::UploadProgress>,
    ) -> Result<UploadResult, UploadError> {
        self.ensure_session().await?;

        let response = self
            .send_upload(file_path, config.folder_id.as_deref(), progress.as_ref())
            .await?;

        let (status_code, body) = login::parse_json(response).await?;
        let json: Value = serde_json::from_str(&body).unwrap_or(Value::Null);

        if login::is_cloudflare_challenge(status_code, &body) {
            return Err(login::cloudflare_error());
        }
        if !(200..300).contains(&status_code) {
            return Err(error_from_value(
                &json,
                status_code,
                "the goonbox upload failed",
            ));
        }

        let parsed: UploadResponse =
            serde_json::from_value(json.clone()).map_err(|e| UploadError {
                message: format!("unreadable goonbox upload response: {e}"),
                status_code: Some(status_code),
            })?;

        let url = parsed
            .image
            .and_then(|image| image.thumb_url)
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| UploadError {
                message: "goonbox stored the image but returned no URL".to_string(),
                status_code: Some(status_code),
            })?;

        Ok(UploadResult {
            urls: vec![url],
            raw_response: Some(json),
        })
    }

    async fn get_or_create_folder(
        &self,
        folder_name: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<Option<String>, UploadError> {
        let albums = self.list_albums().await?;

        let matching: Vec<&Album> = albums
            .iter()
            .filter(|album| album.title.eq_ignore_ascii_case(folder_name))
            .collect();

        match matching.as_slice() {
            [album] => return Ok(Some(album.encoded_id.clone())),
            [] => {}
            many => {
                let ids = many
                    .iter()
                    .map(|album| album.encoded_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(UploadError {
                    message: format!(
                        "{} goonbox albums match '{folder_name}' ({ids}); pass one of those \
                         ids as the folder id",
                        many.len()
                    ),
                    status_code: None,
                });
            }
        }

        Ok(Some(self.create_album(folder_name).await?))
    }

    fn name(&self) -> &str {
        "goonbox"
    }

    fn capabilities(&self) -> UploaderCapabilities {
        Self::CAPABILITIES
    }

    fn max_file_size(&self) -> FileSize {
        Self::MAX_FILE_SIZE
    }

    async fn is_ready(&self) -> bool {
        true
    }
}

impl GoonboxUploader {
    async fn send_upload(
        &self,
        file_path: &str,
        album_id: Option<&str>,
        progress: Option<&upio_core::http::UploadProgress>,
    ) -> Result<reqwest::Response, UploadError> {
        let response = self.post_upload(file_path, album_id, progress).await?;

        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        self.session.invalidate().await;
        self.post_upload(file_path, album_id, progress).await
    }

    async fn post_upload(
        &self,
        file_path: &str,
        album_id: Option<&str>,
        progress: Option<&upio_core::http::UploadProgress>,
    ) -> Result<reqwest::Response, UploadError> {
        let bearer = self.session.bearer().await?;
        // Sanctum answers 419 unless the request echoes the session's CSRF
        // cookie as a header, and the bearer alone is not enough.
        let xsrf = self.session.csrf_header().await?;
        let form = image_form(file_path, album_id, progress).await?;

        self.session
            .client()
            .post(UPLOAD_URL)
            .header("Authorization", format!("Bearer {bearer}"))
            .header(XSRF_HEADER, xsrf)
            .multipart(form)
            .send()
            .await
            .map_err(map_reqwest_error)
    }

    async fn list_albums(&self) -> Result<Vec<Album>, UploadError> {
        self.ensure_session().await?;

        let mut albums = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!("{ALBUMS_URL}?per_page={ALBUMS_PER_PAGE}&page={page}");
            let response = self.authorized_get(&url).await?;
            let (status_code, body) = login::parse_json(response).await?;
            let json: Value = serde_json::from_str(&body).unwrap_or(Value::Null);

            if !(200..300).contains(&status_code) {
                return Err(error_from_value(
                    &json,
                    status_code,
                    "goonbox album lookup failed",
                ));
            }
            let last_page = json
                .get("pagination")
                .and_then(|pagination| pagination.get("last_page"))
                .and_then(Value::as_u64)
                .unwrap_or(page as u64);

            let parsed: AlbumsResponse = serde_json::from_value(json).map_err(|e| UploadError {
                message: format!("unreadable goonbox album list: {e}"),
                status_code: Some(status_code),
            })?;

            let received = parsed.albums.len();
            albums.extend(parsed.albums);

            if received == 0 || page as u64 >= last_page {
                break;
            }
            page += 1;
        }

        Ok(albums)
    }

    async fn create_album(&self, title: &str) -> Result<String, UploadError> {
        self.ensure_session().await?;

        let bearer = self.session.bearer().await?;
        let xsrf = self.session.csrf_header().await?;
        let response = self
            .session
            .client()
            .post(ALBUMS_URL)
            .header("Authorization", format!("Bearer {bearer}"))
            .header(XSRF_HEADER, xsrf)
            .json(&serde_json::json!({ "title": title }))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let (status_code, body) = login::parse_json(response).await?;
        let json: Value = serde_json::from_str(&body).unwrap_or(Value::Null);

        if !(200..300).contains(&status_code) {
            return Err(error_from_value(
                &json,
                status_code,
                "goonbox album creation failed",
            ));
        }

        json.get("album")
            .and_then(|album| album.get("encoded_id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| UploadError {
                message: "goonbox created the album but returned no id".to_string(),
                status_code: Some(status_code),
            })
    }
}

pub(crate) fn error_from_value(value: &Value, status_code: u16, fallback: &str) -> UploadError {
    UploadError {
        message: value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| fallback.to_string()),
        status_code: Some(status_code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_images_are_accepted() {
        let caps = GoonboxUploader::CAPABILITIES;
        assert!(caps.image);
        assert!(!caps.video && !caps.audio && !caps.archives && !caps.arbitrary);
        assert_eq!(caps.names(), vec!["image"]);
    }

    #[test]
    fn the_size_limit_matches_the_server() {
        assert_eq!(GoonboxUploader::MAX_FILE_SIZE, FileSize::from_mb(25));
    }

    #[test]
    fn error_messages_come_from_the_server_when_present() {
        let json = serde_json::json!({ "message": "Invalid album ID" });
        let err = error_from_value(&json, 422, "fallback");
        assert_eq!(err.message, "Invalid album ID");
        assert_eq!(err.status_code, Some(422));

        let blank = error_from_value(&serde_json::json!({}), 500, "fallback");
        assert_eq!(blank.message, "fallback");
    }
}
