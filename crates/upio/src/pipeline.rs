//! High-level upload pipeline: preprocessing + upload in one step.
//!
//! This is the main entry point most consumers should use.
//! Preprocessing rules are read from [`UploaderEndpointConfig::preprocess`]
//! rather than being a hard-coded video-only flag.
//!
//! # Example
//!
//! Any type implementing [`Uploader`] can be driven by the pipeline. Here a
//! minimal in-memory uploader is used so the example runs without a network:
//!
//! ```
//! use async_trait::async_trait;
//! use upio::pipeline;
//! use upio::{
//!     Uploader, UploaderCapabilities, UploaderEndpointConfig, UploadError, UploadResult,
//! };
//!
//! struct Echo;
//!
//! #[async_trait]
//! impl Uploader for Echo {
//!     async fn upload_file(
//!         &self,
//!         file_path: &str,
//!         _config: &UploaderEndpointConfig,
//!     ) -> Result<UploadResult, UploadError> {
//!         Ok(UploadResult { urls: vec![file_path.to_string()], raw_response: None })
//!     }
//!     fn name(&self) -> &str { "echo" }
//!     fn capabilities(&self) -> UploaderCapabilities { UploaderCapabilities::any() }
//!     fn max_file_size(&self) -> upio::filesize::FileSize { upio::filesize::FileSize::MAX }
//!     async fn is_ready(&self) -> bool { true }
//! }
//!
//! # async fn run() {
//! let result = pipeline::upload_file("a.txt", &Echo, &UploaderEndpointConfig::default()).await;
//! assert_eq!(result.urls, vec!["a.txt"]);
//! # }
//! # let _ = run();
//! ```

use futures::stream::{self, StreamExt};
use upio_core::preprocess;
use upio_core::types::{Uploader, UploaderEndpointConfig};

/// Outcome of uploading one source file through the pipeline.
#[derive(Debug)]
pub struct FileResult {
    /// Original source file path.
    pub file: String,
    /// URLs the file was uploaded to (one per successfully uploaded part).
    pub urls: Vec<String>,
    /// If set, at least one part of the file failed to upload.
    pub error: Option<String>,
}

/// Upload a single file through an uploader, applying the endpoint's
/// preprocessing configuration.
///
/// Matching preprocessing rules are applied in order and chain (e.g. normalize
/// the name, then compress, then split what is still too large). Temporary
/// files are cleaned up automatically. The original source path is always
/// reported in the result.
pub async fn upload_file(
    file: &str,
    uploader: &dyn Uploader,
    config: &UploaderEndpointConfig,
) -> FileResult {
    upload_file_with_progress(file, uploader, config, None).await
}

/// Like [`upload_file`], but forwards byte-level progress to `progress`.
pub async fn upload_file_with_progress(
    file: &str,
    uploader: &dyn Uploader,
    config: &UploaderEndpointConfig,
    progress: Option<upio_core::http::UploadProgress>,
) -> FileResult {
    if let Err(e) = uploader.init().await {
        return FileResult {
            file: file.to_owned(),
            urls: Vec::new(),
            error: Some(format!("init failed: {}", e)),
        };
    }

    let preprocess_config = &config.preprocess;

    if !preprocess_config.enabled {
        return match uploader
            .upload_file_with_progress(file, config, progress)
            .await
        {
            Ok(res) if res.urls.is_empty() => FileResult {
                file: file.to_string(),
                urls: vec![],
                error: Some("upload succeeded but the service returned no URL".to_string()),
            },
            Ok(res) => FileResult {
                file: file.to_string(),
                urls: res.urls,
                error: None,
            },
            Err(e) => FileResult {
                file: file.to_string(),
                urls: vec![],
                error: Some(e.to_string()),
            },
        };
    }

    match preprocess::preprocess_file(file, uploader.max_file_size().as_bytes(), preprocess_config)
        .await
    {
        Ok(prepared) => {
            let mut urls = Vec::new();
            let mut failures = Vec::new();

            for part in &prepared.files {
                match uploader
                    .upload_file_with_progress(&part.to_string_lossy(), config, progress.clone())
                    .await
                {
                    Ok(res) if res.urls.is_empty() => {
                        failures.push(format!(
                            "{}: upload succeeded but the service returned no URL",
                            part.display()
                        ));
                    }
                    Ok(res) => urls.extend(res.urls),
                    Err(e) => failures.push(format!("{}: {}", part.display(), e)),
                }
            }

            let cleanup_error = match preprocess::cleanup(&prepared) {
                Ok(()) => None,
                Err(e) => Some(format!("cleanup failed: {}", e)),
            };

            let mut errors = failures;
            if let Some(cleanup_error) = cleanup_error {
                errors.push(cleanup_error);
            }

            if errors.is_empty() {
                FileResult {
                    file: file.to_string(),
                    urls,
                    error: None,
                }
            } else {
                FileResult {
                    file: file.to_string(),
                    urls,
                    error: Some(errors.join("; ")),
                }
            }
        }
        Err(e) => FileResult {
            file: file.to_string(),
            urls: vec![],
            error: Some(format!("preprocess failed: {}", e)),
        },
    }
}

/// Upload a batch of files through a single uploader.
///
/// Preprocessing is handled per file as configured on `config.preprocess`.
/// When `batch_size` is `Some(n)` with `n > 1`, up to `n` files are uploaded
/// concurrently; otherwise files are processed sequentially.
pub async fn upload_files(
    files: &[String],
    uploader: &dyn Uploader,
    config: &UploaderEndpointConfig,
    batch_size: Option<usize>,
) -> Vec<FileResult> {
    match batch_size {
        Some(n) if n > 1 => {
            let futures = files
                .iter()
                .cloned()
                .map(|file| async move { upload_file(&file, uploader, config).await });
            stream::iter(futures)
                .buffered(n.max(1))
                .collect::<Vec<FileResult>>()
                .await
        }
        _ => {
            let mut results = Vec::with_capacity(files.len());
            for file in files {
                results.push(upload_file(file, uploader, config).await);
            }
            results
        }
    }
}
