use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use upio_core::error::UploadError;
use upio_core::filesize::FileSize;
use upio_core::http::{map_reqwest_error, parse_json_response, tracked_file_part};
use upio_core::types::{UploadResult, Uploader, UploaderCapabilities, UploaderEndpointConfig};

#[derive(Deserialize)]
#[allow(dead_code)]
struct FilesterResponse {
    success: bool,
    message: String,
    slug: String,
    file_id: i32,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FilesterFolderListResponse {
    success: bool,
    data: Option<Vec<FilesterFolder>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FilesterFolder {
    id: String,
    name: String,
    public: bool,
    file_count: i32,
    created_at: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FilesterFolderCreateResponse {
    success: bool,
    message: String,
    data: Option<FilesterFolderCreateData>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FilesterFolderCreateData {
    identifier: String,
}

/// Filester.me uploader.
///
/// Construction is synchronous; a token is optional. Use [`FilesterUploader::new`]
/// for anonymous uploads or [`FilesterUploader::with_token`] when you need
/// authenticated features such as folder management. A token can also be
/// supplied per-call via [`UploaderEndpointConfig::token`].
pub struct FilesterUploader {
    client: Client,
    api_key: Option<String>,
}

impl FilesterUploader {
    pub const CAPABILITIES: UploaderCapabilities = UploaderCapabilities::any();
    pub const MAX_FILE_SIZE: FileSize = FileSize::from_gb(10);

    /// Create an uploader without a token. Supply one via
    /// [`FilesterUploader::with_token`] or the upload config.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_key: None,
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: Some(token.to_string()),
        }
    }

    async fn find_folder(
        &self,
        api_key: &str,
        folder_name: &str,
    ) -> Result<Option<String>, UploadError> {
        let resp = self
            .client
            .get("https://u1.filester.me/api/v1/folders")
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key),
            )
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status_code = resp.status().as_u16();
        let folder_resp: FilesterFolderListResponse =
            resp.json().await.map_err(|e| UploadError {
                message: e.to_string(),
                status_code: Some(status_code),
            })?;

        if !folder_resp.success {
            return Err(UploadError {
                message: "Filester returned unsuccessful response when listing folders".to_string(),
                status_code: Some(status_code),
            });
        }

        for f in folder_resp.data.unwrap_or_default() {
            if f.name.eq_ignore_ascii_case(folder_name) {
                return Ok(Some(f.id));
            }
        }

        Ok(None)
    }

    async fn create_folder(&self, api_key: &str, folder_name: &str) -> Result<String, UploadError> {
        let resp = self
            .client
            .post("https://u1.filester.me/api/v1/folder")
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key),
            )
            .json(&serde_json::json!({ "name": folder_name, "public": 1 }))
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status_code = resp.status().as_u16();
        let parsed: FilesterFolderCreateResponse = resp.json().await.map_err(|e| UploadError {
            message: e.to_string(),
            status_code: Some(status_code),
        })?;

        if !parsed.success {
            return Err(UploadError {
                message: format!(
                    "failed to create folder '{}': {}",
                    folder_name, parsed.message
                ),
                status_code: Some(status_code),
            });
        }

        parsed
            .data
            .and_then(|d| {
                if d.identifier.trim().is_empty() {
                    None
                } else {
                    Some(d.identifier)
                }
            })
            .ok_or_else(|| UploadError {
                message: format!(
                    "filester response did not include an identifier for folder '{}'",
                    folder_name
                ),
                status_code: Some(status_code),
            })
    }
}

impl Default for FilesterUploader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Uploader for FilesterUploader {
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
        let part = tracked_file_part(file_path, progress.as_ref()).await?;
        let form = reqwest::multipart::Form::new().part("file", part);
        let mut req = self
            .client
            .post("https://u1.filester.me/api/v1/upload")
            .multipart(form);

        if let Some(folder_id) = &config.folder_id {
            req = req.header("X-Folder-ID", folder_id);
        }
        let api_key = config.token.as_ref().or(self.api_key.as_ref());
        if let Some(key) = api_key {
            req = req.header(reqwest::header::AUTHORIZATION, format!("Bearer {}", key));
        }

        let response = req.send().await.map_err(map_reqwest_error)?;
        let (status_code, raw_response) = parse_json_response(response).await?;

        let parsed: FilesterResponse =
            serde_json::from_value(raw_response.clone()).map_err(|e| UploadError {
                message: format!("invalid Filester response: {}", e),
                status_code: Some(status_code),
            })?;

        if !parsed.success {
            return Err(UploadError {
                message: format!(
                    "Filester returned unsuccessful response: {}",
                    parsed.message
                ),
                status_code: Some(status_code),
            });
        }

        if parsed.slug.trim().is_empty() {
            return Err(UploadError {
                message: "filester response did not include a file slug".to_string(),
                status_code: Some(status_code),
            });
        }

        let urls = vec![format!("https://filester.me/d/{}", parsed.slug)];
        Ok(UploadResult {
            urls,
            raw_response: Some(raw_response),
        })
    }

    async fn get_or_create_folder(
        &self,
        folder_name: &str,
        config: &UploaderEndpointConfig,
    ) -> Result<Option<String>, UploadError> {
        let key = config
            .token
            .as_ref()
            .or(self.api_key.as_ref())
            .ok_or_else(|| UploadError {
                message: "no filester API key set".to_string(),
                status_code: None,
            })?;

        if let Some(id) = self.find_folder(key, folder_name).await? {
            return Ok(Some(id));
        }

        self.create_folder(key, folder_name).await.map(Some)
    }

    fn name(&self) -> &str {
        "filester"
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
