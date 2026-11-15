use crate::download::{BoxedDownloader, ParsedUrl};
use crate::filesize::FileSize;
use crate::types::{BoxedUploader, UploaderCapabilities, UploaderEndpointConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Credential {
    Token,
    OptionalToken,
    Account,
    None,
}

/// The download half of a service: URL classification plus a constructor.
///
/// Downloads are optional per service. A service without this descriptor is
/// upload-only, and its URLs never resolve to it.
#[derive(Clone, Copy)]
pub struct DownloadDef {
    /// Classify a URL as belonging to this service. Pure: no network and no
    /// credentials, so dispatch can happen before a downloader is built.
    pub parse: fn(&str) -> Option<ParsedUrl>,
    /// What the service needs to download. Most services serve public links,
    /// so this may be [`Credential::None`] even when uploads require a token.
    pub credential: Credential,
    pub build: fn(&UploaderEndpointConfig) -> Result<BoxedDownloader, BuildError>,
}

/// Static facts about one service, plus its constructors.
///
/// Each uploader crate declares exactly one of these, so the catalog never
/// repeats a service's name, limits, or credential rules. The `download` half
/// is optional: `None` means the service is upload-only.
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
    pub download: Option<DownloadDef>,
}

#[derive(Debug)]
pub enum BuildError {
    MissingCredential(String),
    Init(String),
    Unsupported(String),
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
            BuildError::Unsupported(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for BuildError {}
