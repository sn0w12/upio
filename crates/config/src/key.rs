use std::fmt;
use std::str::FromStr;

use upio_core::UploaderEndpointConfig;

use crate::config::Config;
use crate::ConfigError;

/// A typed, exhaustive set of editable config keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigKey {
    GlobalDisabledUploaders,
    BunkrToken,
    BunkrFolderId,
    GofileToken,
    GofileFolderId,
    GofileServer,
    FilesterToken,
    FilesterFolderId,
    PixeldrainToken,
    PixeldrainFolderId,
    GoonboxUsername,
    GoonboxPassword,
    GoonboxFolderId,
}

impl ConfigKey {
    pub const ALL: &'static [ConfigKey] = &[
        Self::GlobalDisabledUploaders,
        Self::BunkrToken,
        Self::BunkrFolderId,
        Self::GofileToken,
        Self::GofileFolderId,
        Self::GofileServer,
        Self::FilesterToken,
        Self::FilesterFolderId,
        Self::PixeldrainToken,
        Self::PixeldrainFolderId,
        Self::GoonboxUsername,
        Self::GoonboxPassword,
        Self::GoonboxFolderId,
    ];

    /// The dotted key as written in config files and CLI commands.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GlobalDisabledUploaders => "global.disabled_uploaders",
            Self::BunkrToken => "bunkr.token",
            Self::BunkrFolderId => "bunkr.folder_id",
            Self::GofileToken => "gofile.token",
            Self::GofileFolderId => "gofile.folder_id",
            Self::GofileServer => "gofile.server",
            Self::FilesterToken => "filester.token",
            Self::FilesterFolderId => "filester.folder_id",
            Self::PixeldrainToken => "pixeldrain.token",
            Self::PixeldrainFolderId => "pixeldrain.folder_id",
            Self::GoonboxUsername => "goonbox.username",
            Self::GoonboxPassword => "goonbox.password",
            Self::GoonboxFolderId => "goonbox.folder_id",
        }
    }

    /// Whether this key holds a credential that should be masked by default
    /// in output (e.g. `config get`).
    pub const fn is_secret(self) -> bool {
        matches!(
            self,
            Self::BunkrToken
                | Self::GofileToken
                | Self::FilesterToken
                | Self::PixeldrainToken
                | Self::GoonboxPassword
        )
    }

    /// Read the value from a config.
    pub fn get(self, config: &Config) -> String {
        match self {
            Self::GlobalDisabledUploaders => config.global.disabled_uploaders.join(", "),
            Self::BunkrToken => token_of(&config.bunkr),
            Self::BunkrFolderId => folder_id_of(&config.bunkr),
            Self::GofileToken => token_of(&config.gofile),
            Self::GofileFolderId => folder_id_of(&config.gofile),
            Self::GofileServer => server_of(&config.gofile),
            Self::FilesterToken => token_of(&config.filester),
            Self::FilesterFolderId => folder_id_of(&config.filester),
            Self::PixeldrainToken => token_of(&config.pixeldrain),
            Self::PixeldrainFolderId => folder_id_of(&config.pixeldrain),
            Self::GoonboxUsername => text_of(&config.goonbox, |c| c.username.as_deref()),
            Self::GoonboxPassword => text_of(&config.goonbox, |c| c.password.as_deref()),
            Self::GoonboxFolderId => folder_id_of(&config.goonbox),
        }
    }

    /// Set the value on a config. An empty value or `"none"` clears the field.
    pub fn set(self, config: &mut Config, value: &str) -> Result<(), ConfigError> {
        let empty_or_none = value.is_empty() || value == "none";
        let opt = |v: &str| {
            if empty_or_none {
                None
            } else {
                Some(v.to_string())
            }
        };
        match self {
            Self::GlobalDisabledUploaders => {
                config.global.disabled_uploaders = value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            Self::BunkrToken => endpoint(&mut config.bunkr).token = opt(value),
            Self::BunkrFolderId => endpoint(&mut config.bunkr).folder_id = opt(value),
            Self::GofileToken => endpoint(&mut config.gofile).token = opt(value),
            Self::GofileFolderId => endpoint(&mut config.gofile).folder_id = opt(value),
            Self::GofileServer => endpoint(&mut config.gofile).server = opt(value),
            Self::FilesterToken => endpoint(&mut config.filester).token = opt(value),
            Self::FilesterFolderId => endpoint(&mut config.filester).folder_id = opt(value),
            Self::PixeldrainToken => endpoint(&mut config.pixeldrain).token = opt(value),
            Self::PixeldrainFolderId => endpoint(&mut config.pixeldrain).folder_id = opt(value),
            Self::GoonboxUsername => endpoint(&mut config.goonbox).username = opt(value),
            Self::GoonboxPassword => endpoint(&mut config.goonbox).password = opt(value),
            Self::GoonboxFolderId => endpoint(&mut config.goonbox).folder_id = opt(value),
        }
        Ok(())
    }
}

fn endpoint(section: &mut Option<UploaderEndpointConfig>) -> &mut UploaderEndpointConfig {
    section.get_or_insert_with(Default::default)
}

fn token_of(section: &Option<UploaderEndpointConfig>) -> String {
    section
        .as_ref()
        .and_then(|c| c.token.as_deref())
        .unwrap_or_default()
        .to_string()
}

fn folder_id_of(section: &Option<UploaderEndpointConfig>) -> String {
    section
        .as_ref()
        .and_then(|c| c.folder_id.as_deref())
        .unwrap_or_default()
        .to_string()
}

fn text_of(
    section: &Option<UploaderEndpointConfig>,
    field: impl Fn(&UploaderEndpointConfig) -> Option<&str>,
) -> String {
    let value = section.as_ref().and_then(field);
    value.unwrap_or_default().to_string()
}

fn server_of(section: &Option<UploaderEndpointConfig>) -> String {
    section
        .as_ref()
        .and_then(|c| c.server.as_deref())
        .unwrap_or_default()
        .to_string()
}

impl FromStr for ConfigKey {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|key| key.as_str() == s)
            .ok_or_else(|| ConfigError::InvalidKey(s.to_string()))
    }
}

impl fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
