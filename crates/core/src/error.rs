use std::fmt;

/// Error returned by uploads, downloads, and the code that supports them.
#[derive(Debug, Clone)]
pub struct UpioError {
    pub message: String,
    pub status_code: Option<u16>,
}

impl fmt::Display for UpioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(code) = self.status_code {
            write!(f, "HTTP {}: {}", code, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for UpioError {}

impl From<anyhow::Error> for UpioError {
    fn from(err: anyhow::Error) -> Self {
        UpioError {
            message: err.to_string(),
            status_code: None,
        }
    }
}

/// The original name for [`UpioError`], kept so existing imports and struct
/// literals keep compiling.
pub type UploadError = UpioError;
