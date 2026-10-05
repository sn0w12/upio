//! Pixeldrain (pixeldrain.com) uploader.
//!
//! Pixeldrain's write methods all require an API key, delivered as the password
//! field of HTTP Basic auth; the username is ignored. Uploads go through
//! `PUT /api/file/{name}` with the file as the raw request body, which the API
//! docs recommend over the multipart `POST` variant.
//!
//! Folders are lists. `POST /api/list` needs at least one file, so a list is
//! always created with the id of a file that was just uploaded; `PUT
//! /api/list/{id}` then sets the whole file set, so adding one file means
//! re-sending every id already in the list. Neither endpoint enforces title
//! uniqueness, so name resolution can match more than one list.

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use upio_core::error::UploadError;
use upio_core::filesize::FileSize;
use upio_core::http::{map_io_error, map_reqwest_error};
use upio_core::types::{UploadResult, Uploader, UploaderCapabilities, UploaderEndpointConfig};

const BASE_URL: &str = "https://pixeldrain.com";
const FILE_API: &str = "https://pixeldrain.com/api/file";
const LIST_API: &str = "https://pixeldrain.com/api/list";
const USER_LISTS_URL: &str = "https://pixeldrain.com/api/user/lists";

/// The list title the API assigns when a `PUT` omits one. Sending an update
/// without a title silently renames the list to this, so every update repeats
/// the existing title.
const DEFAULT_LIST_TITLE: &str = "Pixeldrain List";

#[derive(Deserialize)]
struct UploadInfo {
    id: Option<String>,
}

#[derive(Deserialize)]
struct UserList {
    id: String,
    title: String,
}

#[derive(Deserialize)]
struct UserListsResponse {
    lists: Vec<UserList>,
}

#[derive(Deserialize)]
struct ListFile {
    id: String,
}

#[derive(Deserialize)]
struct ListDetail {
    title: Option<String>,
    files: Option<Vec<ListFile>>,
}

#[derive(Deserialize)]
struct CreatedList {
    id: Option<String>,
}

/// Pixeldrain uploader.
///
/// A token is required: the service has no anonymous uploads. Pixeldrain's
/// real filesystem API would be a better fit for folders, but it needs a paid
/// subscription, so lists stand in as folders.
pub struct PixeldrainUploader {
    client: Client,
    api_key: Option<String>,
}

impl PixeldrainUploader {
    pub const CAPABILITIES: UploaderCapabilities = UploaderCapabilities::any();

    /// The free plan reports a 10 GB per-file limit, and the API refuses
    /// anything larger.
    pub const MAX_FILE_SIZE: FileSize = FileSize::from_gb(10);

    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_key: None,
        }
    }

    pub fn with_token(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: Some(api_key.to_string()),
        }
    }

    /// Pixeldrain reads the key from the password half of Basic auth and
    /// ignores the username, so an empty one is sent.
    fn basic_auth(&self, api_key: &str) -> String {
        format!("Basic {}", BASE64.encode(format!(":{api_key}")))
    }

    fn require_token(&self, config: &UploaderEndpointConfig) -> Result<String, UploadError> {
        config
            .token
            .as_deref()
            .or(self.api_key.as_deref())
            .filter(|key| !key.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| UploadError {
                message: "pixeldrain requires an API key (config: pixeldrain.token)".to_string(),
                status_code: None,
            })
    }

    async fn put_file(
        &self,
        api_key: &str,
        name: &str,
        file_path: &str,
        progress: Option<&upio_core::http::UploadProgress>,
    ) -> Result<(u16, Value), UploadError> {
        let file = tokio::fs::File::open(file_path)
            .await
            .map_err(map_io_error)?;
        let total = file.metadata().await.map(|meta| meta.len()).unwrap_or(0);
        let stream = upio_core::http::counted_file_stream(file, total, progress.cloned());
        let body = reqwest::Body::wrap_stream(stream);

        // The name is a path segment, so it has to be percent-encoded; a
        // literal slash would otherwise address a different endpoint.
        let url = format!("{FILE_API}/{}", url_encode(name));

        let response = self
            .client
            .put(url)
            .header("Authorization", self.basic_auth(api_key))
            .header("Content-Type", "application/octet-stream")
            .header(reqwest::header::CONTENT_LENGTH, total.to_string())
            .body(body)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        parse_body(response).await
    }

    async fn list_ids(
        &self,
        api_key: &str,
        list_id: &str,
    ) -> Result<(Option<String>, Vec<String>), UploadError> {
        let response = self
            .client
            .get(format!("{LIST_API}/{list_id}"))
            .header("Authorization", self.basic_auth(api_key))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let (status_code, value) = parse_body(response).await?;
        if !(200..300).contains(&status_code) {
            return Err(error_from_value(
                &value,
                status_code,
                "pixeldrain list lookup failed",
            ));
        }

        let detail: ListDetail = serde_json::from_value(value).map_err(|e| UploadError {
            message: format!("unreadable pixeldrain list: {e}"),
            status_code: Some(status_code),
        })?;

        let ids = detail
            .files
            .unwrap_or_default()
            .into_iter()
            .map(|file| file.id)
            .collect();

        Ok((detail.title, ids))
    }

    /// Sets a list's title and its complete file set. `PUT` replaces the set,
    /// so the ids already in the list have to be sent back or they are dropped.
    async fn write_list(
        &self,
        api_key: &str,
        list_id: &str,
        title: &str,
        file_ids: &[String],
    ) -> Result<(), UploadError> {
        let files: Vec<Value> = file_ids
            .iter()
            .map(|id| serde_json::json!({ "id": id }))
            .collect();

        let response = self
            .client
            .put(format!("{LIST_API}/{list_id}"))
            .header("Authorization", self.basic_auth(api_key))
            .json(&serde_json::json!({ "title": title, "files": files }))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let (status_code, value) = parse_body(response).await?;
        if !(200..300).contains(&status_code) {
            return Err(error_from_value(
                &value,
                status_code,
                "pixeldrain list update failed",
            ));
        }
        Ok(())
    }

    async fn create_list(
        &self,
        api_key: &str,
        title: &str,
        file_id: &str,
    ) -> Result<String, UploadError> {
        let response = self
            .client
            .post(LIST_API)
            .header("Authorization", self.basic_auth(api_key))
            .json(&serde_json::json!({
                "title": title,
                // Owned, not anonymous: an anonymous list is unlinked from the
                // account and never shows up in the list endpoint, which would
                // make name resolution unable to find it again.
                "anonymous": false,
                "files": [{ "id": file_id }],
            }))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let (status_code, value) = parse_body(response).await?;
        if !(200..300).contains(&status_code) {
            return Err(error_from_value(
                &value,
                status_code,
                "pixeldrain list creation failed",
            ));
        }

        let created: CreatedList = serde_json::from_value(value).map_err(|e| UploadError {
            message: format!("unreadable pixeldrain list creation response: {e}"),
            status_code: Some(status_code),
        })?;

        created
            .id
            .filter(|id| !id.is_empty())
            .ok_or_else(|| UploadError {
                message: "pixeldrain created the list but returned no id".to_string(),
                status_code: Some(status_code),
            })
    }

    async fn my_lists(&self, api_key: &str) -> Result<Vec<UserList>, UploadError> {
        let response = self
            .client
            .get(USER_LISTS_URL)
            .header("Authorization", self.basic_auth(api_key))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let (status_code, value) = parse_body(response).await?;
        if !(200..300).contains(&status_code) {
            return Err(error_from_value(
                &value,
                status_code,
                "pixeldrain list listing failed",
            ));
        }

        serde_json::from_value::<UserListsResponse>(value)
            .map(|parsed| parsed.lists)
            .map_err(|e| UploadError {
                message: format!("unreadable pixeldrain list index: {e}"),
                status_code: Some(status_code),
            })
    }
}

impl Default for PixeldrainUploader {
    fn default() -> Self {
        Self::new()
    }
}

fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

async fn parse_body(response: reqwest::Response) -> Result<(u16, Value), UploadError> {
    let status_code = response.status().as_u16();
    let text = response.text().await.map_err(map_reqwest_error)?;
    // A success is `{"success": true, ...}`; a failure carries `value`, a
    // stable machine-readable code, alongside a `message` that may change.
    let value = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok((status_code, value))
}

fn error_from_value(value: &Value, status_code: u16, fallback: &str) -> UploadError {
    let code = value
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or(fallback);

    UploadError {
        message: if code.is_empty() {
            message.to_string()
        } else {
            format!("{message} ({code})")
        },
        status_code: Some(status_code),
    }
}

fn viewer_url(file_id: &str) -> String {
    format!("{BASE_URL}/u/{file_id}")
}

#[async_trait]
impl Uploader for PixeldrainUploader {
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
        let api_key = self.require_token(config)?;
        let name = upio_core::http::file_name_from_path(file_path);

        let (status_code, value) = self
            .put_file(&api_key, &name, file_path, progress.as_ref())
            .await?;

        if !(200..300).contains(&status_code) {
            return Err(error_from_value(
                &value,
                status_code,
                "the pixeldrain upload failed",
            ));
        }

        let info: UploadInfo = serde_json::from_value(value.clone()).map_err(|e| UploadError {
            message: format!("unreadable pixeldrain upload response: {e}"),
            status_code: Some(status_code),
        })?;

        let file_id = info
            .id
            .filter(|id| !id.is_empty())
            .ok_or_else(|| UploadError {
                message: "pixeldrain stored the file but returned no id".to_string(),
                status_code: Some(status_code),
            })?;

        // The list has to be created with a file already in it, so a
        // by-name folder only exists once something has been uploaded into it.
        if let Some(folder) = config.folder_id.as_deref() {
            self.attach_to_list(&api_key, folder, &file_id).await?;
        }

        Ok(UploadResult {
            urls: vec![viewer_url(&file_id)],
            raw_response: Some(value),
        })
    }

    async fn get_or_create_folder(
        &self,
        folder_name: &str,
        config: &UploaderEndpointConfig,
    ) -> Result<Option<String>, UploadError> {
        let api_key = self.require_token(config)?;
        let lists = self.my_lists(&api_key).await?;

        let matching: Vec<&UserList> = lists
            .iter()
            .filter(|list| list.title.eq_ignore_ascii_case(folder_name))
            .collect();

        // Pixeldrain allows duplicate titles, so several matches cannot be
        // resolved to one without guessing where the user wanted the file.
        match matching.as_slice() {
            [list] => return Ok(Some(list.id.clone())),
            [] => {}
            many => {
                let ids = many
                    .iter()
                    .map(|list| list.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(UploadError {
                    message: format!(
                        "{} pixeldrain lists match '{folder_name}' ({ids}); pass one of those \
                         ids as the folder id",
                        many.len()
                    ),
                    status_code: None,
                });
            }
        }

        Ok(Some(self.pending_list_name(&api_key, folder_name).await?))
    }

    fn name(&self) -> &str {
        "pixeldrain"
    }

    fn capabilities(&self) -> UploaderCapabilities {
        Self::CAPABILITIES
    }

    fn max_file_size(&self) -> FileSize {
        Self::MAX_FILE_SIZE
    }

    async fn is_ready(&self) -> bool {
        self.api_key.is_some()
    }
}

impl PixeldrainUploader {
    /// A list cannot exist without a file, so resolving a name that matches
    /// nothing yields the name itself; the first upload creates the list and
    /// the placeholder is never used as an id.
    async fn pending_list_name(
        &self,
        _api_key: &str,
        folder_name: &str,
    ) -> Result<String, UploadError> {
        Ok(folder_name.to_string())
    }

    /// Adds a freshly uploaded file to a folder. `folder_id` is either a real
    /// list id or a title that has not been created yet.
    async fn attach_to_list(
        &self,
        api_key: &str,
        folder: &str,
        file_id: &str,
    ) -> Result<(), UploadError> {
        if let Ok((title, mut ids)) = self.list_ids(api_key, folder).await {
            if ids.iter().any(|id| id == file_id) {
                return Ok(());
            }
            ids.push(file_id.to_string());
            return self
                .write_list(
                    api_key,
                    folder,
                    title.as_deref().unwrap_or(DEFAULT_LIST_TITLE),
                    &ids,
                )
                .await;
        }

        let list_id = self.create_list(api_key, folder, file_id).await?;
        eprintln!("pixeldrain: created list '{folder}' as {list_id}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_kind_is_accepted() {
        let caps = PixeldrainUploader::CAPABILITIES;
        assert!(caps.accepts_any());
        assert_eq!(
            caps.names(),
            vec!["video", "image", "audio", "archives", "any"]
        );
    }

    #[test]
    fn the_size_limit_matches_the_free_plan() {
        assert_eq!(PixeldrainUploader::MAX_FILE_SIZE, FileSize::from_gb(10));
    }

    #[test]
    fn names_are_encoded_as_a_single_path_segment() {
        assert_eq!(url_encode("photo.png"), "photo.png");
        assert_eq!(url_encode("a b.txt"), "a%20b.txt");
        // A raw slash would address a different endpoint entirely.
        assert_eq!(url_encode("dir/file.txt"), "dir%2Ffile.txt");
        assert_eq!(url_encode("100%_done"), "100%25_done");
    }

    #[test]
    fn the_key_travels_in_the_password_field() {
        let uploader = PixeldrainUploader::with_token("secret");
        assert_eq!(uploader.basic_auth("secret"), "Basic OnNlY3JldA==");
    }

    #[test]
    fn server_error_codes_are_surfaced_alongside_the_message() {
        let value = serde_json::json!({
            "success": false,
            "value": "file_too_large",
            "message": "The file you tried to upload is too large",
        });
        let err = error_from_value(&value, 413, "upload failed");
        assert!(
            err.message.contains("file_too_large"),
            "got {}",
            err.message
        );
        assert_eq!(err.status_code, Some(413));
    }

    #[test]
    fn a_missing_message_falls_back_to_the_caller_supplied_one() {
        let err = error_from_value(&serde_json::json!({}), 500, "the upload failed");
        assert_eq!(err.message, "the upload failed");
    }

    #[tokio::test]
    async fn uploading_without_a_token_is_refused_before_any_request() {
        let uploader = PixeldrainUploader::new();
        let err = uploader
            .upload_file("whatever.png", &UploaderEndpointConfig::default())
            .await
            .expect_err("an anonymous upload cannot work");
        assert!(
            err.message.contains("pixeldrain.token"),
            "got {}",
            err.message
        );
    }

    #[test]
    fn viewer_urls_point_at_the_file_page() {
        assert_eq!(viewer_url("abc123"), "https://pixeldrain.com/u/abc123");
    }
}
