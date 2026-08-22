use reqwest::multipart::Part;
use serde_json::Value;

use crate::error::UploadError;

/// Convert a [`reqwest::Error`] into an [`UploadError`], preserving the HTTP
/// status code when one is present.
///
/// ```
/// use upio_core::http::map_reqwest_error;
/// use upio_core::UploadError;
///
/// // `reqwest::Error` is not constructible directly, but this documents the
/// // shape: on a network error the message is kept and status_code is None.
/// fn kind(e: reqwest::Error) -> Option<u16> {
///     map_reqwest_error(e).status_code
/// }
/// ```
pub fn map_reqwest_error(error: reqwest::Error) -> UploadError {
    UploadError {
        message: error.to_string(),
        status_code: error.status().map(|s| s.as_u16()),
    }
}

/// Convert a [`std::io::Error`] into an [`UploadError`] with no status code.
///
/// ```
/// use upio_core::http::map_io_error;
/// use upio_core::UploadError;
///
/// let io = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
/// let err = map_io_error(io);
/// assert_eq!(err.status_code, None);
/// assert!(err.message.contains("missing"));
/// ```
pub fn map_io_error(error: std::io::Error) -> UploadError {
    UploadError {
        message: error.to_string(),
        status_code: None,
    }
}

/// Extract the file name (final path component) from a path string.
///
/// Returns an empty string when the path has no file name.
///
/// ```
/// use upio_core::http::file_name_from_path;
///
/// assert_eq!(file_name_from_path("/tmp/photo.png"), "photo.png");
/// assert_eq!(file_name_from_path("relative/path/video.mp4"), "video.mp4");
/// assert_eq!(file_name_from_path("/"), "");
/// ```
pub fn file_name_from_path(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

/// Open a file and wrap it as a multipart [`Part`] with its file name set.
///
/// Errors (e.g. the file not existing) are mapped to [`UploadError`].
///
/// ```
/// use upio_core::http::make_file_part;
///
/// # async fn run() -> Result<(), upio_core::UploadError> {
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("photo.png");
/// std::fs::write(&path, b"\x89PNG").unwrap();
///
/// // Returns a multipart part for the given file without error.
/// let _part = make_file_part(path.to_str().unwrap()).await?;
/// # Ok(())
/// # }
/// # let _ = run();
/// ```
pub async fn make_file_part(file_path: &str) -> Result<Part, UploadError> {
    let file = tokio::fs::File::open(file_path)
        .await
        .map_err(map_io_error)?;
    Ok(Part::stream(file).file_name(file_name_from_path(file_path)))
}

/// Progress callback receiving cumulative `(uploaded, total)` bytes.
///
/// An owned, cloneable wrapper so progress can be threaded through retry
/// closures that require `'static`.
#[derive(Clone)]
pub struct UploadProgress {
    inner: std::sync::Arc<dyn Fn(u64, u64) + Send + Sync>,
}

impl UploadProgress {
    pub fn new(f: impl Fn(u64, u64) + Send + Sync + 'static) -> Self {
        Self {
            inner: std::sync::Arc::new(f),
        }
    }

    pub fn emit(&self, uploaded: u64, total: u64) {
        (self.inner)(uploaded, total);
    }
}

/// Wrap a tokio file in a byte-counting stream that reports progress through
/// `progress` (if any) as chunks are read.
pub fn counted_file_stream(
    file: tokio::fs::File,
    total: u64,
    progress: Option<UploadProgress>,
) -> impl futures::Stream<Item = std::io::Result<Vec<u8>>> {
    use tokio::io::AsyncReadExt as _;

    futures::stream::unfold(
        (file, 0u64, progress, vec![0u8; CHUNK_SIZE], false),
        move |(mut file, mut uploaded, progress, mut buf, done)| async move {
            if done {
                return None;
            }
            match file.read(&mut buf).await {
                Ok(0) => Some((Ok(Vec::new()), (file, uploaded, progress, buf, true))),
                Ok(n) => {
                    uploaded += n as u64;
                    if let Some(progress) = &progress {
                        progress.emit(uploaded, total);
                    }
                    // Hand the read bytes themselves to the consumer (moving
                    // the buffer, not copying out of it) and continue with a
                    // fresh allocation.
                    buf.truncate(n);
                    let chunk = std::mem::replace(&mut buf, vec![0u8; CHUNK_SIZE]);
                    Some((Ok(chunk), (file, uploaded, progress, buf, false)))
                }
                Err(e) => Some((Err(e), (file, uploaded, progress, buf, true))),
            }
        },
    )
}

/// Read chunk size used by [`counted_file_stream`].
const CHUNK_SIZE: usize = 16 * 1024;

/// Like [`make_file_part`], but the part reports cumulative `(uploaded,
/// total)` bytes through `progress` while it is being read.
pub async fn tracked_file_part(
    file_path: &str,
    progress: Option<&UploadProgress>,
) -> Result<Part, UploadError> {
    let file = tokio::fs::File::open(file_path)
        .await
        .map_err(map_io_error)?;
    let total = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let stream = counted_file_stream(file, total, progress.cloned());
    let body = reqwest::Body::wrap_stream(stream);
    Ok(Part::stream(body).file_name(file_name_from_path(file_path)))
}

/// Consume a [`reqwest::Response`] and parse its body as JSON, returning the
/// status code alongside the parsed value.
///
/// ```
/// use upio_core::http::parse_json_response;
///
/// // Demonstrates the return shape: `(status_code, json_value)`.
/// # fn shape() -> (u16, serde_json::Value) {
/// (200, serde_json::json!({"ok": true}))
/// # }
/// ```
pub async fn parse_json_response(response: reqwest::Response) -> Result<(u16, Value), UploadError> {
    let status_code = response.status().as_u16();
    let json = response
        .json::<Value>()
        .await
        .map_err(|error| UploadError {
            message: error.to_string(),
            status_code: Some(status_code),
        })?;
    Ok((status_code, json))
}
