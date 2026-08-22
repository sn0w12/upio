use std::path::Path;

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::de::DeserializeOwned;

use crate::{Config, ConfigError};

/// A layered, figment-backed reader for configuration.
///
/// Layer precedence (later wins):
///
/// 1. The config file at the given path (skipped if it does not exist).
/// 2. Environment variables prefixed with `UPIO_`, with `_` mapped to `.`
///    (e.g. `UPIO_BUNKR_TOKEN` → `bunkr.token`).
/// 3. Any explicit overrides added via [`Settings::override_str`].
#[derive(Clone)]
pub struct Settings {
    figment: Figment,
}

impl Settings {
    /// Load from the default config location.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&Config::path())
    }

    /// Load from an explicit config file path.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let mut figment = Figment::new();
        if path.is_file() {
            figment = figment.merge(Toml::file_exact(path));
        }
        figment = figment.merge(Env::prefixed("UPIO_").split("_"));
        Ok(Self { figment })
    }

    /// Load from an explicit path, or the default location when `None`.
    pub fn load_from_or_default(path: Option<&Path>) -> Result<Self, ConfigError> {
        match path {
            Some(path) => Self::load_from(path),
            None => Self::load(),
        }
    }

    /// The underlying figment, for consumers that want to layer more sources.
    pub fn figment(&self) -> &Figment {
        &self.figment
    }

    /// Extract the merged configuration.
    pub fn extract(&self) -> Result<Config, ConfigError> {
        Ok(self.figment.extract()?)
    }

    /// Extract an inner value at a dotted path (e.g. `"bunkr.token"`).
    pub fn extract_inner<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, ConfigError> {
        if !self.figment.contains(key) {
            return Ok(None);
        }
        Ok(Some(self.figment.extract_inner(key)?))
    }

    /// Override a key with an explicit value at the highest priority.
    pub fn override_str(&mut self, key: &str, value: &str) {
        let fragment = format!("{} = {:?}", key, value);
        self.figment = self.figment.clone().merge(Toml::string(&fragment));
    }
}
