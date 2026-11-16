//! Catalog of the uploaders compiled into this crate.
//!
//! Every service's facts and constructor live in its own uploader crate as a
//! [`ServiceDef`]. This module only lists the enabled services and resolves
//! names to handles; adding an uploader touches one line here.

use std::fmt;
use std::hash::{Hash, Hasher};

use upio_core::filesize::FileSize;
use upio_core::types::{UploaderCapabilities, UploaderEndpointConfig};
use upio_core::{BoxedDownloader, BoxedUploader, DownloadDef};

pub use upio_core::{BuildError, Credential, ServiceDef};

/// A handle to one compiled-in service.
///
/// Equality and hashing are by descriptor identity, so the same service is
/// always the same handle.
#[derive(Clone, Copy)]
pub struct UploaderId(&'static ServiceDef);

impl UploaderId {
    /// Every compiled-in uploader in a stable order.
    pub const ALL: &'static [UploaderId] = &[
        #[cfg(feature = "bunkr")]
        UploaderId(&uploader_bunkr::BUNKR_SERVICE),
        #[cfg(feature = "gofile")]
        UploaderId(&uploader_gofile::GOFILE_SERVICE),
        #[cfg(feature = "fileditch")]
        UploaderId(&uploader_fileditch::FILEDITCH_SERVICE),
        #[cfg(feature = "filester")]
        UploaderId(&uploader_filester::FILESTER_SERVICE),
        #[cfg(feature = "goonbox")]
        UploaderId(&uploader_goonbox::GOONBOX_SERVICE),
        #[cfg(feature = "pixeldrain")]
        UploaderId(&uploader_pixeldrain::PIXELDRAIN_SERVICE),
    ];

    pub fn def(self) -> &'static ServiceDef {
        self.0
    }

    /// The service name as used in config files, CLI flags, and env vars.
    pub fn name(self) -> &'static str {
        self.0.name
    }

    pub fn capabilities(self) -> UploaderCapabilities {
        self.0.capabilities
    }

    /// The download half of this service, when it has one.
    pub fn download_def(self) -> Option<DownloadDef> {
        self.0.download
    }

    /// Whether this service can resolve its share URLs for downloading.
    pub fn supports_download(self) -> bool {
        self.0.download.is_some()
    }

    /// Resolve a URL to the service that claims it.
    ///
    /// Only services with a download descriptor are considered, so an
    /// upload-only service never intercepts a URL it cannot handle.
    ///
    /// ```
    /// use upio::registry::UploaderId;
    ///
    /// assert_eq!(UploaderId::from_url("https://example.test/file"), None);
    /// ```
    pub fn from_url(url: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|id| {
            id.0.download
                .is_some_and(|download| (download.parse)(url).is_some())
        })
    }

    /// The maximum file size this service accepts, in bytes.
    pub fn max_file_size(self) -> FileSize {
        self.0.max_file_size
    }

    /// Resolve an uploader by its service name.
    ///
    /// ```
    /// use upio::registry::UploaderId;
    ///
    /// let id = UploaderId::from_name("gofile").unwrap();
    /// assert_eq!(id.name(), "gofile");
    /// assert_eq!(UploaderId::from_name("unknown"), None);
    /// ```
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|id| id.name() == name)
    }
}

impl PartialEq for UploaderId {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for UploaderId {}

impl Hash for UploaderId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::hash(self.0, state);
    }
}

impl fmt::Debug for UploaderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.name)
    }
}

/// Build an uploader for the given id from its endpoint config.
///
/// Credential validation is part of each service's descriptor. Construction is
/// synchronous; any network setup is deferred until upload time via `init`.
///
/// # Errors
///
/// Returns [`BuildError::MissingCredential`] when the service requires
/// credentials that are not present in `config`, and [`BuildError::Init`] on
/// construction failure.
///
/// ```
/// use upio::registry::{build_uploader, BuildError, UploaderId};
/// use upio::UploaderEndpointConfig;
///
/// let err = match build_uploader(UploaderId::from_name("bunkr").unwrap(), &UploaderEndpointConfig::default()) {
///     Ok(_) => panic!("bunkr built without a token"),
///     Err(e) => e,
/// };
/// assert!(matches!(err, BuildError::MissingCredential(_)));
/// ```
pub fn build_uploader(
    id: UploaderId,
    config: &UploaderEndpointConfig,
) -> Result<BoxedUploader, BuildError> {
    (id.0.build)(config)
}

/// Build a downloader for the given id from its endpoint config.
///
/// # Errors
///
/// Returns [`BuildError::Unsupported`] when the service has no download half,
/// [`BuildError::MissingCredential`] when its download rules require a
/// credential that is absent, and [`BuildError::Init`] on construction failure.
///
/// ```
/// use upio::registry::{build_downloader, UploaderId};
/// use upio::UploaderEndpointConfig;
///
/// let id = UploaderId::from_name("goonbox").unwrap();
/// let downloader = build_downloader(id, &UploaderEndpointConfig::default()).unwrap();
/// assert_eq!(downloader.name(), "goonbox");
/// ```
pub fn build_downloader(
    id: UploaderId,
    config: &UploaderEndpointConfig,
) -> Result<BoxedDownloader, BuildError> {
    match id.0.download {
        Some(download) => (download.build)(config),
        None => Err(BuildError::Unsupported(format!(
            "{} does not support downloads",
            id.name()
        ))),
    }
}
