//! Central registry of known uploaders.
//!
//! This is the single source of truth for which uploaders exist, their names,
//! their capabilities, and how to construct them. Consumers (the CLI, a future
//! GUI, tests) should use [`UploaderId`] and [`build_uploader`] instead of
//! hard-coding per-service logic.

use upio_core::{
    filesize::FileSize, BoxedUploader, Uploader, UploaderCapabilities, UploaderEndpointConfig,
};

/// A known uploader service.
///
/// The `Bunkr` variant is gated on the `bunkr` feature, and so on, so the set of
/// compile-time-known uploaders matches the enabled feature flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UploaderId {
    #[cfg(feature = "bunkr")]
    Bunkr,
    #[cfg(feature = "gofile")]
    Gofile,
    #[cfg(feature = "fileditch")]
    Fileditch,
    #[cfg(feature = "filester")]
    Filester,
}

impl UploaderId {
    /// Every known uploader in a stable order.
    pub const ALL: &'static [UploaderId] = &[
        #[cfg(feature = "bunkr")]
        UploaderId::Bunkr,
        #[cfg(feature = "gofile")]
        UploaderId::Gofile,
        #[cfg(feature = "fileditch")]
        UploaderId::Fileditch,
        #[cfg(feature = "filester")]
        UploaderId::Filester,
    ];

    /// The service name as used in config files, CLI flags, and env vars.
    pub const fn name(self) -> &'static str {
        match self {
            #[cfg(feature = "bunkr")]
            UploaderId::Bunkr => "bunkr",
            #[cfg(feature = "gofile")]
            UploaderId::Gofile => "gofile",
            #[cfg(feature = "fileditch")]
            UploaderId::Fileditch => "fileditch",
            #[cfg(feature = "filester")]
            UploaderId::Filester => "filester",
        }
    }

    /// The kinds of files this uploader accepts.
    pub fn capabilities(self) -> UploaderCapabilities {
        match self {
            #[cfg(feature = "bunkr")]
            UploaderId::Bunkr => uploader_bunkr::BunkrUploader::CAPABILITIES,
            #[cfg(feature = "gofile")]
            UploaderId::Gofile => uploader_gofile::GoFileUploader::CAPABILITIES,
            #[cfg(feature = "fileditch")]
            UploaderId::Fileditch => uploader_fileditch::FileditchUploader::CAPABILITIES,
            #[cfg(feature = "filester")]
            UploaderId::Filester => uploader_filester::FilesterUploader::CAPABILITIES,
        }
    }

    /// The maximum file size this service accepts, in bytes.
    ///
    /// Constructors are synchronous and cheap, so this builds a throwaway
    /// uploader to query it.
    pub fn max_file_size(self) -> FileSize {
        match self {
            // Use function since bunkr max file size is dynamic
            #[cfg(feature = "bunkr")]
            UploaderId::Bunkr => uploader_bunkr::BunkrUploader::with_token("").max_file_size(),
            #[cfg(feature = "gofile")]
            UploaderId::Gofile => uploader_gofile::GoFileUploader::MAX_FILE_SIZE,
            #[cfg(feature = "fileditch")]
            UploaderId::Fileditch => uploader_fileditch::FileditchUploader::MAX_FILE_SIZE,
            #[cfg(feature = "filester")]
            UploaderId::Filester => uploader_filester::FilesterUploader::MAX_FILE_SIZE,
        }
    }

    /// Resolve an uploader by its service name.
    ///
    /// ```
    /// use upio::registry::UploaderId;
    ///
    /// let id = UploaderId::from_name("gofile");
    /// assert!(matches!(id, Some(UploaderId::Gofile)));
    /// assert_eq!(UploaderId::from_name("unknown"), None);
    /// ```
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|id| id.name() == name)
    }
}

/// Errors produced while constructing an uploader.
#[derive(Debug)]
pub enum BuildError {
    /// The name does not match any known uploader.
    Unknown(String),
    /// The uploader requires a token that is not configured.
    MissingToken(String),
    /// The uploader's constructor failed.
    Init(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Unknown(name) => write!(f, "unknown uploader '{}'", name),
            BuildError::MissingToken(name) => {
                write!(
                    f,
                    "no token for '{}' (set config or UPIO_{}_TOKEN)",
                    name,
                    name.to_uppercase()
                )
            }
            BuildError::Init(msg) => write!(f, "failed to init uploader: {}", msg),
        }
    }
}

impl std::error::Error for BuildError {}

/// Build an uploader for the given id from its endpoint config.
///
/// Tokens required by a service are read from `config.token`. Construction is
/// synchronous; any network setup is deferred until upload time via
/// [`Uploader::init`]. On success the boxed uploader implements
/// [`upio_core::Uploader`] and can be driven by the shared pipeline.
///
/// # Errors
///
/// Returns [`BuildError::Unknown`] for an id that is not compiled in,
/// [`BuildError::MissingToken`] when the service requires a token that is not
/// present in `config`, and [`BuildError::Init`] on construction failure.
///
/// ```
/// use upio::registry::{build_uploader, BuildError, UploaderId};
/// use upio::UploaderEndpointConfig;
///
/// // Bunkr requires a token.
/// let err = match build_uploader(UploaderId::Bunkr, &UploaderEndpointConfig::default()) {
///     Ok(_) => panic!("bunkr built without a token"),
///     Err(e) => e,
/// };
/// assert!(matches!(err, BuildError::MissingToken(_)));
/// ```
pub fn build_uploader(
    id: UploaderId,
    config: &UploaderEndpointConfig,
) -> Result<BoxedUploader, BuildError> {
    let _ = (id, config);
    match id {
        #[cfg(feature = "bunkr")]
        UploaderId::Bunkr => {
            let token = config
                .token
                .as_deref()
                .ok_or_else(|| BuildError::MissingToken(id.name().to_string()))?;
            Ok(Box::new(uploader_bunkr::BunkrUploader::with_token(token)))
        }
        #[cfg(feature = "gofile")]
        UploaderId::Gofile => {
            let uploader = match config.token.as_deref() {
                Some(token) => uploader_gofile::GoFileUploader::with_token(token),
                None => uploader_gofile::GoFileUploader::new(),
            };
            Ok(Box::new(uploader))
        }
        #[cfg(feature = "fileditch")]
        UploaderId::Fileditch => Ok(Box::new(uploader_fileditch::FileditchUploader::new())),
        #[cfg(feature = "filester")]
        UploaderId::Filester => {
            let uploader = match config.token.as_deref() {
                Some(token) => uploader_filester::FilesterUploader::with_token(token),
                None => uploader_filester::FilesterUploader::new(),
            };
            Ok(Box::new(uploader))
        }
    }
}
