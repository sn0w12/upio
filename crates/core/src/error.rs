use std::fmt;

#[derive(Debug, Clone)]
pub struct UploadError {
    pub message: String,
    pub status_code: Option<u16>,
}

impl fmt::Display for UploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(code) = self.status_code {
            write!(f, "HTTP {}: {}", code, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for UploadError {}

impl From<anyhow::Error> for UploadError {
    fn from(err: anyhow::Error) -> Self {
        UploadError {
            message: err.to_string(),
            status_code: None,
        }
    }
}
