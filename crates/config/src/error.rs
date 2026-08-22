use std::fmt;

use figment::Error as FigmentError;
use toml::de::Error as TomlDeError;
use toml::ser::Error as TomlSerError;

/// Errors produced while reading, merging, or writing configuration.
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    TomlDe(TomlDeError),
    TomlSer(TomlSerError),
    Figment(Box<FigmentError>),
    InvalidKey(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "io error: {}", e),
            ConfigError::TomlDe(e) => write!(f, "failed to parse config: {}", e),
            ConfigError::TomlSer(e) => write!(f, "failed to serialize config: {}", e),
            ConfigError::Figment(e) => write!(f, "config error: {}", e),
            ConfigError::InvalidKey(key) => write!(f, "unknown config key: '{}'", key),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io(e) => Some(e),
            ConfigError::TomlDe(e) => Some(e),
            ConfigError::TomlSer(e) => Some(e),
            ConfigError::Figment(e) => Some(e),
            ConfigError::InvalidKey(_) => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<TomlDeError> for ConfigError {
    fn from(e: TomlDeError) -> Self {
        ConfigError::TomlDe(e)
    }
}

impl From<TomlSerError> for ConfigError {
    fn from(e: TomlSerError) -> Self {
        ConfigError::TomlSer(e)
    }
}

impl From<FigmentError> for ConfigError {
    fn from(e: FigmentError) -> Self {
        ConfigError::Figment(Box::new(e))
    }
}
