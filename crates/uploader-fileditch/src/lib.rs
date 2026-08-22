use async_trait::async_trait;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use upio_core::error::UploadError;
use upio_core::filesize::FileSize;
use upio_core::http::counted_file_stream;
use upio_core::http::{file_name_from_path, map_io_error, map_reqwest_error, parse_json_response};
use upio_core::types::{UploadResult, Uploader, UploaderCapabilities, UploaderEndpointConfig};

#[derive(Deserialize)]
struct FileditchResponse {
    success: bool,
    url: String,
}

#[derive(Deserialize)]
struct FileditchErrorResponse {
    error: String,
}

fn parse_upload_response(
    raw_response: Value,
    status_code: u16,
) -> Result<UploadResult, UploadError> {
    if let Ok(err) = serde_json::from_value::<FileditchErrorResponse>(raw_response.clone()) {
        return Err(UploadError {
            message: err.error,
            status_code: Some(status_code),
        });
    }

    let parsed: FileditchResponse =
        serde_json::from_value(raw_response.clone()).map_err(|e| UploadError {
            message: format!("invalid fileditch response: {}", e),
            status_code: Some(status_code),
        })?;

    if !parsed.success {
        return Err(UploadError {
            message: "fileditch returned unsuccessful response".to_string(),
            status_code: Some(status_code),
        });
    }

    if parsed.url.trim().is_empty() {
        return Err(UploadError {
            message: "fileditch upload succeeded but no file URL was returned".to_string(),
            status_code: Some(status_code),
        });
    }

    Ok(UploadResult {
        urls: vec![parsed.url],
        raw_response: Some(raw_response),
    })
}

/// Fileditch.com uploader.
///
/// This service requires no authentication token, so only [`FileditchUploader::new`]
/// is provided. Implements [`Uploader`] and can be driven by the shared
/// pipeline.
pub struct FileditchUploader {
    client: Client,
}

impl FileditchUploader {
    pub const CAPABILITIES: UploaderCapabilities = UploaderCapabilities::any();
    pub const MAX_FILE_SIZE: FileSize = FileSize::from_gb(25);

    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for FileditchUploader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Uploader for FileditchUploader {
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
        _config: &UploaderEndpointConfig,
        progress: Option<upio_core::http::UploadProgress>,
    ) -> Result<UploadResult, UploadError> {
        let file_name = file_name_from_path(file_path);

        let file = tokio::fs::File::open(file_path)
            .await
            .map_err(map_io_error)?;
        let file_size = file.metadata().await.map_err(map_io_error)?;

        if file_size.len() == 0 {
            return Err(UploadError {
                message: "empty files are not accepted".to_string(),
                status_code: Some(400),
            });
        }

        let total = file_size.len();
        let body = reqwest::Body::wrap_stream(counted_file_stream(file, total, progress));

        let response = self
            .client
            .post("https://new.fileditch.com/upload.php")
            .query(&[("filename", file_name.as_str())])
            .header(CONTENT_TYPE, "application/octet-stream")
            .header(CONTENT_LENGTH, file_size.len().to_string())
            .body(body)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let (status_code, raw_response) = parse_json_response(response).await?;
        parse_upload_response(raw_response, status_code)
    }

    fn name(&self) -> &str {
        "fileditch"
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
