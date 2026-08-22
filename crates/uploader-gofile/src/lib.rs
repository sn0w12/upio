use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use upio_core::error::UploadError;
use upio_core::filesize::FileSize;
use upio_core::http::{map_reqwest_error, parse_json_response, tracked_file_part};
use upio_core::types::{UploadResult, Uploader, UploaderCapabilities, UploaderEndpointConfig};

const SERVERS_URL: &str = "https://api.gofile.io/servers";
const CONTENT_URL: &str = "https://api.gofile.io/contents";
const ACCOUNTS_URL: &str = "https://api.gofile.io/accounts";

#[derive(Debug, Deserialize, Clone)]
pub struct Server {
    pub name: String,
    pub zone: String,
}

#[derive(Deserialize)]
struct ServersResponse {
    #[serde(rename = "serversAllZone")]
    servers_all_zone: Vec<Server>,
}

#[derive(Debug, Deserialize)]
struct UploadData {
    #[serde(rename = "downloadPage")]
    download_page: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountIdData {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AccountData {
    #[serde(rename = "rootFolder")]
    root_folder: String,
}

/// A child (folder or file) of a folder's contents.
#[derive(Debug, Deserialize)]
struct Content {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    name: String,
}

/// The `data` payload of a `GET /contents/{id}` response: the children keyed
/// by their id.
#[derive(Debug, Deserialize)]
struct ContentData {
    #[serde(default)]
    children: std::collections::HashMap<String, Content>,
}

#[derive(Debug, Deserialize)]
struct CreateFolderData {
    id: String,
}

fn response_handler(response: &serde_json::Value) -> Result<serde_json::Value, UploadError> {
    if response["status"] == "ok" {
        Ok(response["data"].clone())
    } else {
        let status = response["status"].as_str().unwrap_or("invalid response");
        let message = status.split_once('-').map(|(_, msg)| msg).unwrap_or(status);
        Err(UploadError {
            message: message.to_string(),
            status_code: None,
        })
    }
}

/// GoFile.io uploader.
///
/// Construction is synchronous; a token is optional. Use [`GoFileUploader::new`]
/// for anonymous uploads or [`GoFileUploader::with_token`] when you need
/// authenticated features such as folder management. A token can also be
/// supplied per-call via [`UploaderEndpointConfig::token`].
pub struct GoFileUploader {
    client: Client,
    token: Option<String>,
}

impl GoFileUploader {
    pub const CAPABILITIES: UploaderCapabilities = UploaderCapabilities::any();
    pub const MAX_FILE_SIZE: FileSize = FileSize::MAX;

    /// Create an uploader without a token. Supply one via
    /// [`GoFileUploader::with_token`] or the upload config.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            token: None,
        }
    }

    /// Create an uploader with a token.
    pub fn with_token(token: &str) -> Self {
        Self {
            client: Client::new(),
            token: Some(token.to_string()),
        }
    }

    /// Pick a server for the requested zone, falling back to any available.
    async fn pick_server(&self, zone: &str) -> Result<Server, UploadError> {
        let response = self
            .client
            .get(SERVERS_URL)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let (_, json) = parse_json_response(response).await?;
        let data = response_handler(&json)?;
        let servers: ServersResponse = serde_json::from_value(data).map_err(|e| UploadError {
            message: e.to_string(),
            status_code: None,
        })?;
        let servers = servers.servers_all_zone;
        servers
            .iter()
            .find(|s| s.zone == zone)
            .or_else(|| servers.first())
            .cloned()
            .ok_or_else(|| UploadError {
                message: "noServersAvailable".to_string(),
                status_code: None,
            })
    }

    /// The UUID of the account's root folder, which is the parent of every
    /// top-level folder.
    async fn account_root_folder(&self, token: &str) -> Result<String, UploadError> {
        let response = self
            .client
            .get(format!("{}/getid", ACCOUNTS_URL))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let (_, json) = parse_json_response(response).await?;
        let data = response_handler(&json)?;
        let account: AccountIdData =
            serde_json::from_value(data.clone()).map_err(|e| UploadError {
                message: format!("invalid getid response: {}", e),
                status_code: None,
            })?;

        let response = self
            .client
            .get(format!("{}/{}", ACCOUNTS_URL, account.id))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let (_, json) = parse_json_response(response).await?;
        let data = response_handler(&json)?;
        let account: AccountData = serde_json::from_value(data).map_err(|e| UploadError {
            message: format!("invalid account response: {}", e),
            status_code: None,
        })?;
        Ok(account.root_folder)
    }

    /// Read a folder's metadata and direct children.
    async fn read_content(
        &self,
        token: &str,
        content_id: &str,
    ) -> Result<ContentData, UploadError> {
        let response = self
            .client
            .get(format!("{}/{}", CONTENT_URL, content_id))
            .query(&[("page", "1"), ("pageSize", "1000")])
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let (_, json) = parse_json_response(response).await?;
        let data = response_handler(&json)?;
        serde_json::from_value(data).map_err(|e| UploadError {
            message: format!("invalid contents response: {}", e),
            status_code: None,
        })
    }

    /// Search the folder tree under `root_id` for a folder whose name matches
    /// `folder_name` (case-insensitive), using an iterative depth-first walk.
    async fn find_folder(
        &self,
        token: &str,
        root_id: &str,
        folder_name: &str,
    ) -> Result<Option<String>, UploadError> {
        let mut stack = vec![root_id.to_string()];
        while let Some(id) = stack.pop() {
            let content = self.read_content(token, &id).await?;
            for child in content.children.values() {
                if child.kind != "folder" {
                    continue;
                }
                if child.name.eq_ignore_ascii_case(folder_name) {
                    return Ok(Some(child.id.clone()));
                }
                stack.push(child.id.clone());
            }
        }
        Ok(None)
    }

    /// Create a folder inside a parent folder.
    async fn create_folder(
        &self,
        token: &str,
        parent_id: &str,
        folder_name: &str,
    ) -> Result<String, UploadError> {
        let response = self
            .client
            .post(format!("{}/createFolder", CONTENT_URL))
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "parentFolderId": parent_id,
                "folderName": folder_name,
                "public": true,
            }))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let (_, json) = parse_json_response(response).await?;
        let data = response_handler(&json)?;
        let created: CreateFolderData = serde_json::from_value(data).map_err(|e| UploadError {
            message: format!("invalid createFolder response: {}", e),
            status_code: None,
        })?;
        Ok(created.id)
    }

    async fn upload(
        &self,
        file_path: &str,
        token: Option<&str>,
        folder_id: Option<&str>,
        server: Option<&str>,
        progress: Option<upio_core::http::UploadProgress>,
    ) -> Result<serde_json::Value, UploadError> {
        let server = match server {
            Some(name) => name.to_string(),
            None => self.pick_server("eu").await?.name,
        };
        let part = tracked_file_part(file_path, progress.as_ref()).await?;
        let mut form = reqwest::multipart::Form::new().part("file", part);
        if let Some(fid) = folder_id {
            form = form.text("folderId", fid.to_string());
        }
        let mut request = self
            .client
            .post(format!("https://{}.gofile.io/uploadFile", server))
            .multipart(form);
        if let Some(t) = token {
            request = request.header("Authorization", format!("Bearer {}", t));
        }
        let response = request.send().await.map_err(|e| UploadError {
            message: e.to_string(),
            status_code: e.status().map(|s| s.as_u16()),
        })?;
        let (_, json) = parse_json_response(response).await?;
        response_handler(&json)
    }
}

impl Default for GoFileUploader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Uploader for GoFileUploader {
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
        let token = config.token.as_deref().or(self.token.as_deref());
        let response = self
            .upload(
                file_path,
                token,
                config.folder_id.as_deref(),
                config.server.as_deref(),
                progress,
            )
            .await?;

        let data: UploadData =
            serde_json::from_value(response.clone()).map_err(|e| UploadError {
                message: format!("invalid gofile response: {}", e),
                status_code: None,
            })?;

        let url = data
            .download_page
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| UploadError {
                message: "gofile response did not include a download page URL".to_string(),
                status_code: None,
            })?;

        Ok(UploadResult {
            urls: vec![url],
            raw_response: Some(response),
        })
    }

    async fn get_or_create_folder(
        &self,
        folder_name: &str,
        config: &UploaderEndpointConfig,
    ) -> Result<Option<String>, UploadError> {
        let token = config
            .token
            .as_deref()
            .or(self.token.as_deref())
            .ok_or_else(|| UploadError {
                message: "gofile requires a token to manage folders".to_string(),
                status_code: None,
            })?;

        let root = self.account_root_folder(token).await?;

        // Return the existing folder's UUID if one already exists.
        if let Some(id) = self.find_folder(token, &root, folder_name).await? {
            return Ok(Some(id));
        }

        // Otherwise create a top-level folder with that name.
        let id = self.create_folder(token, &root, folder_name).await?;
        Ok(Some(id))
    }
    fn name(&self) -> &str {
        "gofile"
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
