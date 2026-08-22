use std::path::Path;

use async_trait::async_trait;
use reqwest::multipart;
use reqwest::{Body, Client};
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::sync::OnceCell;
use tokio::time::{sleep, Duration};
use upio_core::http::UploadProgress;

use upio_core::error::UploadError;
use upio_core::filesize::FileSize;
use upio_core::http::file_name_from_path;
use upio_core::types::{UploadResult, Uploader, UploaderCapabilities, UploaderEndpointConfig};
use uuid::Uuid;

const VERIFY_URL: &str = "https://dash.bunkr.cr/api/tokens/verify";
const CONFIG_URL: &str = "https://dash.bunkr.cr/api/check";
const NODE_URL: &str = "https://dash.bunkr.cr/api/node";
const NODE_REFRESH_RETRIES: u32 = 6;

#[derive(Debug, Deserialize)]
struct NodeResponse {
    success: bool,
    url: String,
}

#[derive(Debug, Deserialize)]
struct UploadResponse {
    success: bool,
    files: Option<Vec<UploadedFile>>,
}

#[derive(Debug, Deserialize)]
struct UploadedFile {
    url: String,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct BunkrServerConfig {
    #[allow(dead_code)]
    maxSize: String,
    chunkSize: ChunkSizeConfig,
}

#[derive(Debug, Deserialize)]
struct ChunkSizeConfig {
    #[allow(dead_code)]
    max: String,
    default: String,
}

/// Server-derived settings needed to upload, fetched lazily once.
#[derive(Clone)]
struct ServerConfig {
    upload_url: String,
    max_file_size: u64,
    chunk_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct Album {
    pub id: i64,
    pub name: String,
}

/// Bunkr.cr uploader.
///
/// Construction is synchronous via [`BunkrUploader::with_token`]; the token is
/// not verified and server capabilities are not fetched until the first upload
/// (see [`Uploader::init`]). Implements [`Uploader`] and can be driven by the
/// shared pipeline.
pub struct BunkrUploader {
    client: Client,
    token: String,
    server: OnceCell<ServerConfig>,
}

impl BunkrUploader {
    pub const CAPABILITIES: UploaderCapabilities = UploaderCapabilities::any();
    pub const MAX_FILE_SIZE: FileSize = FileSize::from_gb(2);

    /// Create an uploader for the given API token. Network verification is
    /// deferred to the first upload (see [`Uploader::init`]).
    pub fn with_token(token: &str) -> Self {
        Self {
            client: Client::new(),
            token: token.to_string(),
            server: OnceCell::new(),
        }
    }

    /// The number of retry attempts used for transient network failures.
    const RETRIES: u32 = 5;

    async fn server_config(&self) -> Result<&ServerConfig, UploadError> {
        self.server
            .get_or_try_init(|| async {
                self.verify_token().await?;
                let (max_file_size, chunk_size) = self.fetch_capabilities().await?;
                let upload_url = self.fetch_upload_url().await?;
                Ok(ServerConfig {
                    upload_url,
                    max_file_size,
                    chunk_size,
                })
            })
            .await
    }

    async fn verify_token(&self) -> Result<(), UploadError> {
        #[derive(Deserialize)]
        struct VerifyResponse {
            success: bool,
        }
        let response = self
            .retry(|| {
                let client = self.client.clone();
                let token = self.token.clone();
                async move {
                    client
                        .post(VERIFY_URL)
                        .form(&[("token", &token)])
                        .send()
                        .await
                        .map_err(Box::from)
                }
            })
            .await?;
        let status = response.status();
        let text = response.text().await.map_err(map_reqwest_err)?;
        if !status.is_success() {
            return Err(UploadError {
                message: format!("Token verification failed with status {}: {}", status, text),
                status_code: Some(status.as_u16()),
            });
        }
        let verify: VerifyResponse = serde_json::from_str(&text).map_err(|e| UploadError {
            message: format!("Failed to parse verify response: {}", e),
            status_code: None,
        })?;
        if !verify.success {
            return Err(UploadError {
                message: "Invalid API token".to_string(),
                status_code: None,
            });
        }
        Ok(())
    }

    async fn fetch_capabilities(&self) -> Result<(u64, u64), UploadError> {
        let response = self
            .retry(|| {
                let client = self.client.clone();
                let token = self.token.clone();
                async move {
                    client
                        .get(CONFIG_URL)
                        .header("token", &token)
                        .send()
                        .await
                        .map_err(Box::from)
                }
            })
            .await?;
        let status = response.status();
        let text = response.text().await.map_err(map_reqwest_err)?;
        if !status.is_success() {
            return Err(UploadError {
                message: format!("Config fetch failed with status {}: {}", status, text),
                status_code: Some(status.as_u16()),
            });
        }
        let config: BunkrServerConfig = serde_json::from_str(&text).map_err(|e| UploadError {
            message: format!("Failed to parse server config: {}", e),
            status_code: None,
        })?;
        let raw_max = parse_size(&config.maxSize)?;
        let max_file_size = (raw_max as f64 * 0.95) as u64;
        let chunk_size = parse_size(&config.chunkSize.default)?;
        if max_file_size == 0 {
            return Err(UploadError {
                message: "server reported a zero max file size".to_string(),
                status_code: None,
            });
        }
        if chunk_size == 0 {
            return Err(UploadError {
                message: "server reported a zero chunk size".to_string(),
                status_code: None,
            });
        }
        Ok((max_file_size, chunk_size))
    }

    async fn fetch_upload_url(&self) -> Result<String, UploadError> {
        let response = self
            .retry(|| {
                let client = self.client.clone();
                let token = self.token.clone();
                async move {
                    client
                        .get(NODE_URL)
                        .header("token", &token)
                        .send()
                        .await
                        .map_err(Box::from)
                }
            })
            .await?;
        let status = response.status();
        let text = response.text().await.map_err(map_reqwest_err)?;
        if !status.is_success() {
            return Err(UploadError {
                message: format!("Node fetch failed with status {}: {}", status, text),
                status_code: Some(status.as_u16()),
            });
        }
        let node: NodeResponse = serde_json::from_str(&text).map_err(|e| UploadError {
            message: format!("Failed to parse node response: {}", e),
            status_code: None,
        })?;
        if !node.success {
            return Err(UploadError {
                message: "Node fetch failed: server returned success=false".to_string(),
                status_code: None,
            });
        }
        Ok(node.url)
    }

    /// Retry a request with exponential backoff on transient failures.
    async fn retry<F, Fut>(&self, f: F) -> Result<reqwest::Response, UploadError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<
            Output = Result<reqwest::Response, Box<dyn std::error::Error + Send + Sync>>,
        >,
    {
        let mut delay = Duration::from_secs(1);
        let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for attempt in 0..=Self::RETRIES {
            match f().await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_error = Some(e);
                    if attempt == Self::RETRIES {
                        break;
                    }
                    sleep(delay).await;
                    delay = delay.saturating_mul(2);
                }
            }
        }
        let error = last_error.expect("retry loop runs at least once");
        Err(UploadError {
            message: error.to_string(),
            status_code: None,
        })
    }

    /// Upload a single file. Returns the file's URL on success; a successful
    /// response that carries no file URL is an error, not `None`.
    async fn upload_file_inner(
        &self,
        path: &str,
        album_id: Option<&str>,
        progress: Option<&UploadProgress>,
    ) -> Result<String, UploadError> {
        let server = self.server_config().await?;

        let p = Path::new(path);
        let metadata = p.metadata().map_err(|e| UploadError {
            message: format!("failed to stat '{}': {}", path, e),
            status_code: None,
        })?;
        let size = metadata.len();
        let mime = mime_guess::from_path(p).first_or_octet_stream();

        // Try uploading; if the node URL is stale, refresh it and retry.
        let mut current_upload_url = server.upload_url.clone();
        for attempt in 0..=NODE_REFRESH_RETRIES {
            let is_final = attempt == NODE_REFRESH_RETRIES;
            let result = if size <= server.chunk_size {
                self.upload_single(
                    p,
                    mime.essence_str(),
                    album_id,
                    &current_upload_url,
                    is_final,
                    progress,
                )
                .await?
            } else {
                self.upload_chunked(
                    p,
                    mime.essence_str(),
                    album_id,
                    &current_upload_url,
                    is_final,
                    progress,
                )
                .await?
            };

            if result.is_some() || is_final {
                return result.ok_or_else(|| UploadError {
                    message: "upload succeeded but the service returned no file URL".to_string(),
                    status_code: None,
                });
            }
            current_upload_url = self.fetch_upload_url().await?;
        }

        unreachable!("the loop always returns on its final attempt")
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload_single(
        &self,
        path: &Path,
        mime: &str,
        album_id: Option<&str>,
        upload_url: &str,
        record_failure: bool,
        progress: Option<&UploadProgress>,
    ) -> Result<Option<String>, UploadError> {
        let file_name = file_name_from_path(&path.to_string_lossy());
        let mut headers = reqwest::header::HeaderMap::new();
        let token_header = self.token.parse().map_err(|e| UploadError {
            message: format!("invalid token header: {}", e),
            status_code: None,
        })?;
        headers.insert("token", token_header);
        if let Some(aid) = album_id {
            let album_header = aid.parse().map_err(|e| UploadError {
                message: format!("invalid album id header: {}", e),
                status_code: None,
            })?;
            headers.insert("albumid", album_header);
        }

        let response = self
            .retry(|| {
                let file_name = file_name.clone();
                let headers = headers.clone();
                let path = path.to_path_buf();
                let upload_url = upload_url.to_string();
                let client = self.client.clone();
                async move {
                    let file = tokio::fs::File::open(&path)
                        .await
                        .map_err(Box::<dyn std::error::Error + Send + Sync>::from)?;
                    let total = file.metadata().await.map(|m| m.len()).unwrap_or(0);
                    let stream =
                        upio_core::http::counted_file_stream(file, total, progress.cloned());
                    let body = Body::wrap_stream(stream);
                    let part = multipart::Part::stream(body)
                        .file_name(file_name)
                        .mime_str(mime)
                        .map_err(Box::<dyn std::error::Error + Send + Sync>::from)?;
                    let form = multipart::Form::new().part("files[]", part);
                    client
                        .post(&upload_url)
                        .headers(headers)
                        .multipart(form)
                        .send()
                        .await
                        .map_err(Box::from)
                }
            })
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.map_err(map_reqwest_err)?;
            return if record_failure {
                Err(UploadError {
                    message: format!("Upload failed with status {}: {}", status, text),
                    status_code: Some(status.as_u16()),
                })
            } else {
                Ok(None)
            };
        }

        let text = response.text().await.map_err(map_reqwest_err)?;
        let res: UploadResponse = serde_json::from_str(&text).map_err(|e| UploadError {
            message: format!("Failed to parse response: {}", e),
            status_code: None,
        })?;
        if !res.success {
            return if record_failure {
                Err(UploadError {
                    message: "Upload failed: server returned success=false".to_string(),
                    status_code: None,
                })
            } else {
                Ok(None)
            };
        }

        Ok(res.files.and_then(|f| f.first().map(|x| x.url.clone())))
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload_chunked(
        &self,
        path: &Path,
        mime: &str,
        album_id: Option<&str>,
        upload_url: &str,
        record_failure: bool,
        progress: Option<&UploadProgress>,
    ) -> Result<Option<String>, UploadError> {
        let server = self.server_config().await?;
        let total_size = path.metadata().map_err(map_io_err)?.len();
        // Ceiling division in checked arithmetic; a zero chunk size is
        // rejected when the server config is fetched.
        let total_chunks = total_size
            .checked_add(server.chunk_size - 1)
            .and_then(|sum| sum.checked_div(server.chunk_size))
            .ok_or_else(|| UploadError {
                message: "chunk count overflow for this file size".to_string(),
                status_code: None,
            })?;
        let file_name = file_name_from_path(&path.to_string_lossy());
        let uuid = Uuid::new_v4();

        let mut file = tokio::fs::File::open(path).await.map_err(map_io_err)?;
        let mut buf = Vec::with_capacity(server.chunk_size as usize);
        let mut uploaded_so_far = 0u64;

        for i in 0..total_chunks {
            buf.clear();
            let mut total_read = 0usize;
            while total_read < server.chunk_size as usize {
                let remaining = server.chunk_size as usize - total_read;
                buf.resize(total_read + remaining, 0);
                let n = file
                    .read(&mut buf[total_read..])
                    .await
                    .map_err(map_io_err)?;
                if n == 0 {
                    break;
                }
                total_read += n;
            }
            buf.truncate(total_read);

            let chunk_offset = i
                .checked_mul(server.chunk_size)
                .ok_or_else(|| UploadError {
                    message: "chunk offset overflow".to_string(),
                    status_code: None,
                })?;

            let response = self
                .retry(|| {
                    let buf = buf.clone();
                    let file_name = file_name.clone();
                    let upload_url = upload_url.to_string();
                    let client = self.client.clone();
                    let token = self.token.clone();
                    let chunk_index = i;
                    let chunk_size = server.chunk_size;
                    async move {
                        let part = multipart::Part::bytes(buf)
                            .file_name(file_name)
                            .mime_str("application/octet-stream")?;
                        let form = multipart::Form::new()
                            .text("dzuuid", uuid.to_string())
                            .text("dzchunkindex", chunk_index.to_string())
                            .text("dztotalfilesize", total_size.to_string())
                            .text("dzchunksize", chunk_size.to_string())
                            .text("dztotalchunkcount", total_chunks.to_string())
                            .text("dzchunkbyteoffset", chunk_offset.to_string())
                            .part("files[]", part);
                        client
                            .post(&upload_url)
                            .header("token", &token)
                            .multipart(form)
                            .send()
                            .await
                            .map_err(Box::from)
                    }
                })
                .await?;

            uploaded_so_far += total_read as u64;
            if let Some(progress) = progress {
                progress.emit(uploaded_so_far, total_size);
            }

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.map_err(map_reqwest_err)?;
                if record_failure {
                    return Err(UploadError {
                        message: format!(
                            "Chunk {} upload failed with status {}: {}",
                            i, status, text
                        ),
                        status_code: Some(status.as_u16()),
                    });
                }
                return Ok(None);
            }
        }

        let albumid_value = match album_id.and_then(|id| id.parse::<i64>().ok()) {
            Some(id) => serde_json::Value::Number(serde_json::Number::from(id)),
            None => serde_json::Value::Null,
        };
        let body = json!({
            "files": [{
                "uuid": uuid.to_string(),
                "original": file_name,
                "type": mime,
                "albumid": albumid_value,
                "filelength": null,
                "age": null,
            }]
        });

        let finish_url = format!("{}/finishchunks", upload_url);
        let response = self
            .retry(|| {
                let body = body.clone();
                let client = self.client.clone();
                let token = self.token.clone();
                let finish_url = finish_url.clone();
                async move {
                    client
                        .post(&finish_url)
                        .header("token", &token)
                        .json(&body)
                        .send()
                        .await
                        .map_err(Box::from)
                }
            })
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.map_err(map_reqwest_err)?;
            return if record_failure {
                Err(UploadError {
                    message: format!("Finish chunks failed with status {}: {}", status, text),
                    status_code: Some(status.as_u16()),
                })
            } else {
                Ok(None)
            };
        }

        let text = response.text().await.map_err(map_reqwest_err)?;
        let res: UploadResponse = serde_json::from_str(&text).map_err(|e| UploadError {
            message: format!("Failed to parse finish response: {}", e),
            status_code: None,
        })?;
        if !res.success {
            return if record_failure {
                Err(UploadError {
                    message: "Finish chunks failed: server returned success=false".to_string(),
                    status_code: None,
                })
            } else {
                Ok(None)
            };
        }

        Ok(res.files.and_then(|f| f.first().map(|x| x.url.clone())))
    }

    pub async fn get_albums(&self) -> Result<Vec<Album>, UploadError> {
        #[derive(Deserialize)]
        struct AlbumsResponse {
            albums: Vec<Album>,
        }
        let response = self
            .retry(|| {
                let client = self.client.clone();
                let token = self.token.clone();
                async move {
                    client
                        .get("https://dash.bunkr.cr/api/albums")
                        .header("token", &token)
                        .send()
                        .await
                        .map_err(Box::from)
                }
            })
            .await?;
        let status = response.status();
        let text = response.text().await.map_err(map_reqwest_err)?;
        if !status.is_success() {
            return Err(UploadError {
                message: format!("Albums fetch failed with status {}: {}", status, text),
                status_code: Some(status.as_u16()),
            });
        }
        let res: AlbumsResponse = serde_json::from_str(&text).map_err(|e| UploadError {
            message: format!("Failed to parse albums response: {}", e),
            status_code: None,
        })?;
        Ok(res.albums)
    }

    pub async fn get_album_by_name(&self, album_name: &str) -> Result<Option<i64>, UploadError> {
        let albums = self.get_albums().await?;
        Ok(albums
            .into_iter()
            .find(|album| album.name.eq_ignore_ascii_case(album_name))
            .map(|album| album.id))
    }

    pub async fn create_album(
        &self,
        name: String,
        description: Option<String>,
        download: bool,
        public: bool,
    ) -> Result<i64, UploadError> {
        let body = json!({
            "name": name,
            "description": description.unwrap_or_default(),
            "download": download,
            "public": public,
        });
        let response = self
            .retry(|| {
                let body = body.clone();
                let client = self.client.clone();
                let token = self.token.clone();
                async move {
                    client
                        .post("https://dash.bunkr.cr/api/albums")
                        .header("token", &token)
                        .json(&body)
                        .send()
                        .await
                        .map_err(Box::from)
                }
            })
            .await?;
        let status = response.status();
        let text = response.text().await.map_err(map_reqwest_err)?;
        if !status.is_success() {
            return Err(UploadError {
                message: format!("Create album failed with status {}: {}", status, text),
                status_code: Some(status.as_u16()),
            });
        }
        let res: serde_json::Value = serde_json::from_str(&text).map_err(|e| UploadError {
            message: format!("Failed to parse create album response: {}", e),
            status_code: None,
        })?;
        if res["success"] == true {
            res["id"].as_i64().ok_or_else(|| UploadError {
                message: "Create album response missing id".to_string(),
                status_code: None,
            })
        } else {
            let message = res["description"]
                .as_str()
                .map(|d| d.to_string())
                .unwrap_or_else(|| "Create album failed: success=false".to_string());
            Err(UploadError {
                message,
                status_code: None,
            })
        }
    }
}

#[async_trait]
impl Uploader for BunkrUploader {
    async fn init(&self) -> Result<(), UploadError> {
        self.server_config().await.map(|_| ())
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
        progress: Option<UploadProgress>,
    ) -> Result<UploadResult, UploadError> {
        self.server_config().await?;
        let url = self
            .upload_file_inner(file_path, config.folder_id.as_deref(), progress.as_ref())
            .await?;
        Ok(UploadResult {
            urls: vec![url],
            raw_response: None,
        })
    }

    async fn get_or_create_folder(
        &self,
        folder_name: &str,
        _config: &UploaderEndpointConfig,
    ) -> Result<Option<String>, UploadError> {
        // Return the existing album's id if a matching one already exists.
        if let Some(id) = self.get_album_by_name(folder_name).await? {
            return Ok(Some(id.to_string()));
        }

        // Otherwise create a new album with that name.
        let id = self
            .create_album(folder_name.to_string(), None, true, true)
            .await?;
        Ok(Some(id.to_string()))
    }

    fn name(&self) -> &str {
        "bunkr"
    }

    fn capabilities(&self) -> UploaderCapabilities {
        Self::CAPABILITIES
    }

    fn max_file_size(&self) -> FileSize {
        self.server
            .get()
            .map(|s| FileSize::from_bytes(s.max_file_size))
            .unwrap_or(FileSize::from_gb(2))
    }

    async fn is_ready(&self) -> bool {
        self.server.get().is_some()
    }
}

fn parse_size(size_str: &str) -> Result<u64, UploadError> {
    let s = size_str.trim().to_uppercase();
    let (num, multiplier) = if let Some(n) = s.strip_suffix("GB") {
        (n, 1_000_000_000u64)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1_000_000)
    } else if let Some(n) = s.strip_suffix("KB") {
        (n, 1_000)
    } else if let Some(n) = s.strip_suffix("B") {
        (n, 1)
    } else {
        return Err(UploadError {
            message: format!("Invalid size format: {}", size_str),
            status_code: None,
        });
    };
    let n: u64 = num.trim().parse().map_err(|e| UploadError {
        message: format!("Invalid size format '{}': {}", size_str, e),
        status_code: None,
    })?;
    n.checked_mul(multiplier).ok_or_else(|| UploadError {
        message: format!("Size '{}' overflows", size_str),
        status_code: None,
    })
}

fn map_reqwest_err(e: reqwest::Error) -> UploadError {
    UploadError {
        message: e.to_string(),
        status_code: e.status().map(|s| s.as_u16()),
    }
}

fn map_io_err(e: std::io::Error) -> UploadError {
    UploadError {
        message: e.to_string(),
        status_code: None,
    }
}
