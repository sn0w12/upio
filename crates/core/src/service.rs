use crate::filesize::FileSize;
use crate::types::{BoxedUploader, UploaderCapabilities, UploaderEndpointConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Credential {
    Token,
    OptionalToken,
    Account,
    None,
}

/// Static facts about one uploader service, plus its constructor.
///
/// Each uploader crate declares exactly one of these, so the catalog never
/// repeats a service's name, limits, or credential rules.
#[derive(Clone, Copy)]
pub struct ServiceDef {
    pub name: &'static str,
    pub display_name: &'static str,
    pub crate_name: &'static str,
    pub credential: Credential,
    pub supports_folders: bool,
    pub capabilities: UploaderCapabilities,
    pub max_file_size: FileSize,
    pub build: fn(&UploaderEndpointConfig) -> Result<BoxedUploader, BuildError>,
}

#[derive(Debug)]
pub enum BuildError {
    MissingCredential(String),
    Init(String),
}

impl BuildError {
    pub fn missing_token(name: &str) -> Self {
        Self::MissingCredential(format!(
            "no token for '{}' (set config or UPIO_{}_TOKEN)",
            name,
            name.to_uppercase()
        ))
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::MissingCredential(message) => f.write_str(message),
            BuildError::Init(msg) => write!(f, "failed to init uploader: {}", msg),
        }
    }
}

impl std::error::Error for BuildError {}
