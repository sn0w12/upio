use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use upio_core::download::ConflictPolicy;
use upio_core::{PreprocessConfig, UploaderEndpointConfig};

use crate::key::ConfigKey;
use crate::ConfigError;

/// The on-disk configuration model.
///
/// All fields default via serde, so partially-populated files parse cleanly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    /// Default preprocessing rules for every service. A `[service.preprocess]`
    /// table replaces this configuration for that service.
    #[serde(default, skip_serializing_if = "is_default_preprocess")]
    pub preprocess: PreprocessConfig,
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
    #[serde(default)]
    pub pixeldrain: Option<UploaderEndpointConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub disabled_uploaders: Vec<String>,
    #[serde(default)]
    pub disabled_downloaders: Vec<String>,
    #[serde(default)]
    pub download: DownloadSettings,
}

/// Defaults applied when `upio download` is invoked without explicit flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSettings {
    /// Where files are written. Unset means the current directory.
    #[serde(default)]
    pub output_dir: Option<PathBuf>,
    #[serde(default = "default_download_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_true")]
    pub resume: bool,
    #[serde(default)]
    pub on_conflict: ConflictPolicy,
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            output_dir: None,
            concurrency: default_download_concurrency(),
            resume: true,
            on_conflict: ConflictPolicy::default(),
        }
    }
}

fn default_download_concurrency() -> usize {
    4
}

fn default_true() -> bool {
    true
}

fn is_default_preprocess(config: &PreprocessConfig) -> bool {
    !config.enabled && config.rules.is_empty()
}

/// Whether an uploader is enabled: exact, allocation-free comparison of its
/// canonical name against `global.disabled_uploaders`. Unknown disabled names
/// are inert; only exact canonical names disable a service.
pub fn is_uploader_enabled(disabled: &[String], name: &str) -> bool {
    !disabled.iter().any(|d| d == name)
}

/// Whether a downloader is enabled, with the same exact-name matching rules as
/// [`is_uploader_enabled`].
pub fn is_downloader_enabled(disabled: &[String], name: &str) -> bool {
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
            "pixeldrain" => self.pixeldrain.clone().unwrap_or_default(),
            _ => UploaderEndpointConfig::default(),
        }
    }

    /// The service's own `[service.preprocess]` table, when present.
    pub fn preprocess_override(&self, name: &str) -> Option<&PreprocessConfig> {
        match name {
            "bunkr" => self.bunkr.as_ref(),
            "gofile" => self.gofile.as_ref(),
            "fileditch" => self.fileditch.as_ref(),
            "filester" => self.filester.as_ref(),
            "goonbox" => self.goonbox.as_ref(),
            "pixeldrain" => self.pixeldrain.as_ref(),
            _ => None,
        }
        .and_then(|endpoint| endpoint.preprocess.as_ref())
    }

    /// Effective preprocessing for a service: its override when set,
    /// otherwise the global configuration.
    pub fn preprocess_for(&self, name: &str) -> PreprocessConfig {
        self.preprocess_override(name)
            .cloned()
            .unwrap_or_else(|| self.preprocess.clone())
    }

    /// The endpoint for a service with preprocessing resolved, ready for the
    /// upload pipeline.
    pub fn endpoint_for(&self, name: &str) -> UploaderEndpointConfig {
        let mut endpoint = self.get_uploader_config(name);
        endpoint.preprocess = Some(self.preprocess_for(name));
        endpoint
    }

    /// Mutable preprocessing override for a service, seeded from the global
    /// rules on first edit.
    pub fn preprocess_mut(&mut self, name: &str) -> Option<&mut PreprocessConfig> {
        let global = self.preprocess.clone();
        let endpoint = match name {
            "bunkr" => self.bunkr.get_or_insert_with(Default::default),
            "gofile" => self.gofile.get_or_insert_with(Default::default),
            "fileditch" => self.fileditch.get_or_insert_with(Default::default),
            "filester" => self.filester.get_or_insert_with(Default::default),
            "goonbox" => self.goonbox.get_or_insert_with(Default::default),
            "pixeldrain" => self.pixeldrain.get_or_insert_with(Default::default),
            _ => return None,
        };
        Some(endpoint.preprocess.get_or_insert(global))
    }

    /// Drop a service's preprocessing override, making it inherit the global
    /// rules again. Returns whether an override was present.
    pub fn clear_preprocess_override(&mut self, name: &str) -> bool {
        match name {
            "bunkr" => self.bunkr.as_mut(),
            "gofile" => self.gofile.as_mut(),
            "fileditch" => self.fileditch.as_mut(),
            "filester" => self.filester.as_mut(),
            "goonbox" => self.goonbox.as_mut(),
            "pixeldrain" => self.pixeldrain.as_mut(),
            _ => None,
        }
        .and_then(|endpoint| endpoint.preprocess.take())
        .is_some()
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
