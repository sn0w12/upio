//! `upio` unified file upload library.
//!
//! A single-crate dependency for consuming all uploader functionality.
//! Enable only the uploaders you need via feature flags.
//!
//! # Quick start
//!
//! ```ignore
//! use upio::{BunkrUploader, UploaderEndpointConfig, pipeline};
//!
//! let uploader = BunkrUploader::with_token("token");
//! let config = UploaderEndpointConfig { folder_id: Some("...".into()), ..Default::default() };
//! let result = pipeline::upload_file(&files[0], &uploader, &config).await;
//! ```

pub mod pipeline;
pub mod registry;

pub use upio_core::*;

#[cfg(feature = "bunkr")]
pub use uploader_bunkr::*;
#[cfg(feature = "fileditch")]
pub use uploader_fileditch::*;
#[cfg(feature = "filester")]
pub use uploader_filester::*;
#[cfg(feature = "gofile")]
pub use uploader_gofile::*;
#[cfg(feature = "goonbox")]
pub use uploader_goonbox::*;
#[cfg(feature = "pixeldrain")]
pub use uploader_pixeldrain::*;
