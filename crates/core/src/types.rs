use serde_json::Value;

use crate::error::UploadError;
use crate::filesize::FileSize;
use crate::preprocess::PreprocessConfig;

/// Per-endpoint configuration for a single hosting service.
///
/// This carries everything a particular uploader needs to talk to its API:
/// authentication token, destination folder, and any preprocessing rules. Each
/// uploader implementation reads the fields it supports and ignores the rest.
///
/// ```
/// use upio_core::UploaderEndpointConfig;
///
/// let config = UploaderEndpointConfig {
///     token: Some("secret".to_string()),
///     folder_id: Some("1234ab".to_string()),
///     ..Default::default()
/// };
/// assert_eq!(config.token.as_deref(), Some("secret"));
/// ```
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct UploaderEndpointConfig {
    /// Authentication token for the service.
    pub token: Option<String>,
    /// Destination folder / album ID.
    pub folder_id: Option<String>,
    /// Specific server to use (service-dependent); auto-selected when unset.
    pub server: Option<String>,
    /// Preprocessing rules applied to files before they are uploaded through
    /// this endpoint. See [`crate::preprocess`].
    #[serde(default)]
    pub preprocess: PreprocessConfig,
}

/// Describes which kinds of files an uploader accepts.
///
/// A general-purpose host (e.g. GoFile, Filester) sets every flag and
/// `arbitrary`, while a specialised service (e.g. an image host) sets only the
/// kinds it supports. Consumers should treat a `true` on any flag as an
/// "accepts at least this kind" signal and treat `arbitrary` as "accepts
/// anything that does not match a named kind".
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UploaderCapabilities {
    pub video: bool,
    pub image: bool,
    pub audio: bool,
    pub archives: bool,
    /// Accepts files that do not fall into a named kind.
    pub arbitrary: bool,
}

impl UploaderCapabilities {
    /// A general-purpose host that accepts any file.
    ///
    /// ```
    /// use upio_core::UploaderCapabilities;
    ///
    /// let caps = UploaderCapabilities::any();
    /// assert!(caps.accepts_any());
    /// assert!(caps.video && caps.image && caps.audio && caps.archives);
    /// ```
    pub const fn any() -> Self {
        Self {
            video: true,
            image: true,
            audio: true,
            archives: true,
            arbitrary: true,
        }
    }

    pub const fn none() -> Self {
        Self {
            video: false,
            image: false,
            audio: false,
            archives: false,
            arbitrary: false,
        }
    }

    /// Whether this uploader accepts files of any kind.
    ///
    /// ```
    /// use upio_core::UploaderCapabilities;
    ///
    /// let general = UploaderCapabilities { arbitrary: true, ..Default::default() };
    /// let image_only = UploaderCapabilities { image: true, ..Default::default() };
    /// assert!(general.accepts_any());
    /// assert!(!image_only.accepts_any());
    /// ```
    pub fn accepts_any(&self) -> bool {
        self.arbitrary
    }

    /// Human-readable names for the accepted kinds, for display purposes.
    ///
    /// ```
    /// use upio_core::UploaderCapabilities;
    ///
    /// let caps = UploaderCapabilities { video: true, image: true, ..Default::default() };
    /// assert_eq!(caps.names(), vec!["video", "image"]);
    /// ```
    pub fn names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.video {
            names.push("video");
        }
        if self.image {
            names.push("image");
        }
        if self.audio {
            names.push("audio");
        }
        if self.archives {
            names.push("archives");
        }
        if self.arbitrary {
            names.push("any");
        }
        names
    }
}

/// The outcome of uploading one file through a service.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// The public URL(s) the file is reachable at, one per uploaded part.
    pub urls: Vec<String>,
    /// The raw JSON response body, retained for callers that need details the
    /// uploader did not surface.
    pub raw_response: Option<Value>,
}

pub type BoxedUploader = Box<dyn Uploader>;

#[async_trait::async_trait]
pub trait Uploader: Send + Sync {
    /// Perform any one-time, potentially network-bound setup (e.g. verifying a
    /// token or discovering service capabilities). Called by the pipeline
    /// before uploading; the default does nothing.
    async fn init(&self) -> Result<(), UploadError> {
        Ok(())
    }

    /// Upload a single file and return its result.
    ///
    /// `config` provides per-call settings (token, folder, server). Must be
    /// safe to call concurrently from multiple tasks.
    async fn upload_file(
        &self,
        file_path: &str,
        config: &UploaderEndpointConfig,
    ) -> Result<UploadResult, UploadError>;

    /// Upload with optional byte-level progress reporting.
    ///
    /// The default delegates to [`Uploader::upload_file`] and ignores
    /// progress; streaming uploaders override this to emit cumulative
    /// `(uploaded_bytes, total_bytes)` while the request body is sent.
    async fn upload_file_with_progress(
        &self,
        file_path: &str,
        config: &UploaderEndpointConfig,
        progress: Option<crate::http::UploadProgress>,
    ) -> Result<UploadResult, UploadError> {
        let _ = progress;
        self.upload_file(file_path, config).await
    }

    /// Resolve `folder_name` to a folder ID, creating the folder if it does
    /// not already exist. Returns `None` when the service does not support
    /// folders.
    ///
    /// `config` provides per-call settings (e.g. a token that differs from the
    /// one the uploader was constructed with). The default does nothing and
    /// reports no folder support.
    async fn get_or_create_folder(
        &self,
        folder_name: &str,
        config: &UploaderEndpointConfig,
    ) -> Result<Option<String>, UploadError> {
        let _ = (folder_name, config);
        Ok(None)
    }

    /// The service name (e.g. `"bunkr"`, `"gofile"`), used for config keys and
    /// display.
    fn name(&self) -> &str;

    /// The kinds of files this uploader accepts.
    fn capabilities(&self) -> UploaderCapabilities;

    /// The maximum file size (in bytes) this service accepts. Used by
    /// preprocessing to decide whether to split or compress. Defaults to
    /// unbounded.
    fn max_file_size(&self) -> FileSize;
    /// Whether the uploader is ready to accept uploads.
    async fn is_ready(&self) -> bool;
}
