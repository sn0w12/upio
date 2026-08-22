//! Configuration handling for the upio ecosystem.
//!
//! This crate owns the on-disk configuration format and the layered,
//! [figment]-backed reader.
//!
//! Two complementary surfaces are provided:
//!
//! - [`Config`] the pure data model. Load, edit, and persist the TOML config
//!   file (`Config::from_file`, `Config::to_file`, `ConfigKey`).
//! - [`Settings`] a figment-backed layered reader for running. Layers are
//!   merged with later layers taking precedence: the config file, then
//!   `UPIO_*` environment variables (`UPIO_BUNKR_TOKEN` →
//!   `bunkr.token`), then any explicit overrides.
//!
//! # Layering from another consumer
//!
//! A GUI can layer its own in-memory overrides on top of the file and env
//! sources and extract the same [`Config`]:
//!
//! ```ignore
//! use figment::{Figment, providers::Serialized};
//! use upio_config::Settings;
//!
//! let settings = Settings::load()?;
//! let figment = Figment::new()
//!     .merge(settings.figment().clone())
//!     .merge(Serialized::defaults(overrides)); // GUI-provided values win
//! let config: upio_config::Config = figment.extract()?;
//! ```

pub mod config;
pub mod error;
pub mod key;
pub mod settings;

pub use config::{is_uploader_enabled, write_atomic, Config, GlobalConfig};
pub use error::ConfigError;
pub use key::ConfigKey;
pub use settings::Settings;
pub use upio_core::UploaderEndpointConfig;
