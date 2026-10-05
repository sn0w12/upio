use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use upio_core::UploaderEndpointConfig;

use crate::key::ConfigKey;
use crate::ConfigError;

/// The on-disk configuration model.
///
/// All fields default via serde, so partially-populated files parse cleanly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub bunkr: Option<UploaderEndpointConfig>,
    #[serde(default)]
    pub gofile: Option<UploaderEndpointConfig>,
    #[serde(default)]
    pub fileditch: Option<UploaderEndpointConfig>,
    #[serde(default)]
    pub filester: Option<UploaderEndpointConfig>,
    #[serde(default)]
    pub goonbox: Option<UploaderEndpointConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub disabled_uploaders: Vec<String>,
}

/// Whether an uploader is enabled: exact, allocation-free comparison of its
/// canonical name against `global.disabled_uploaders`. Unknown disabled names
/// are inert; only exact canonical names disable a service.
pub fn is_uploader_enabled(disabled: &[String], name: &str) -> bool {
    !disabled.iter().any(|d| d == name)
}

impl Config {
    /// The default config file location, e.g. `~/.config/upio/config.toml`.
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("upio")
            .join("config.toml")
    }

    /// Load the config from the default location, or a default config if the
    /// file does not exist yet.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::path();
        if path.is_file() {
            Self::from_file(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Serialize this config to a file atomically: the bytes are written to a
    /// sibling temp file, flushed, and then renamed over `path`. A reader
    /// therefore sees either the previous complete file or the new complete
    /// file — never a truncated one.
    pub fn to_file(&self, path: &Path) -> Result<(), ConfigError> {
        let contents = self.to_toml()?;
        write_atomic(path, contents.as_bytes())
    }

    /// Serialize this config to a TOML string.
    ///
    /// ```
    /// use upio_config::{Config, ConfigKey};
    ///
    /// let mut config = Config::default();
    /// config.set_value(&ConfigKey::BunkrToken, "abc").unwrap();
    /// let toml = config.to_toml().unwrap();
    /// assert!(toml.contains("token"));
    /// ```
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Persist to the default location.
    pub fn save(&self) -> Result<(), ConfigError> {
        self.to_file(&Self::path())
    }

    /// Parse a config from a file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    /// The endpoint config for a service name (`bunkr`, `gofile`, ...).
    pub fn get_uploader_config(&self, name: &str) -> UploaderEndpointConfig {
        match name {
            "bunkr" => self.bunkr.clone().unwrap_or_default(),
            "gofile" => self.gofile.clone().unwrap_or_default(),
            "fileditch" => self.fileditch.clone().unwrap_or_default(),
            "filester" => self.filester.clone().unwrap_or_default(),
            "goonbox" => self.goonbox.clone().unwrap_or_default(),
            _ => UploaderEndpointConfig::default(),
        }
    }

    /// Read a value by [`ConfigKey`].
    ///
    /// ```
    /// use upio_config::{Config, ConfigKey};
    ///
    /// let mut config = Config::default();
    /// config.set_value(&ConfigKey::BunkrToken, "abc").unwrap();
    /// assert_eq!(config.get_value(&ConfigKey::BunkrToken), "abc");
    /// ```
    pub fn get_value(&self, key: &ConfigKey) -> String {
        key.get(self)
    }

    /// Set a value by [`ConfigKey`]. An empty value or `"none"` clears the key.
    ///
    /// ```
    /// use upio_config::{Config, ConfigKey};
    ///
    /// let mut config = Config::default();
    /// config.set_value(&ConfigKey::GofileServer, "store4").unwrap();
    /// assert_eq!(config.gofile.as_ref().unwrap().server.as_deref(), Some("store4"));
    /// config.set_value(&ConfigKey::GofileServer, "none").unwrap();
    /// assert_eq!(config.gofile.as_ref().unwrap().server, None);
    /// ```
    pub fn set_value(&mut self, key: &ConfigKey, value: &str) -> Result<(), ConfigError> {
        key.set(self, value)
    }
}

/// Atomically replace `path`'s contents with `contents`: the bytes are
/// written to a sibling temp file, flushed, and then renamed over the
/// destination. A reader therefore sees either the previous complete file or
/// the new complete file — never a truncated one.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    temp.write_all(contents)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| ConfigError::Io(e.error))?;
    Ok(())
}
