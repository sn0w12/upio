use std::path::Path;

use anyhow::Result;

use upio_config::{Config, ConfigKey};

use crate::cli::ConfigAction;

/// Run the `config` command: get, set, or print the config path.
pub fn run(action: ConfigAction, config_path: Option<&Path>) -> Result<()> {
    match action {
        ConfigAction::Get {
            key,
            show_secrets: false,
        } => {
            let config = load_config(config_path)?;
            match key {
                Some(key) => {
                    let key = key.parse::<ConfigKey>()?;
                    let value = config.get_value(&key);
                    println!("{}", mask_if_secret(key, &value));
                }
                None => println!("{}", masked_toml(&config)?),
            }
        }
        ConfigAction::Get {
            key,
            show_secrets: true,
        } => {
            let config = load_config(config_path)?;
            match key {
                Some(key) => println!("{}", config.get_value(&key.parse::<ConfigKey>()?)),
                None => println!("{}", config.to_toml()?),
            }
        }
        ConfigAction::Set { key, value } => {
            let key = key.parse::<ConfigKey>()?;
            let mut config = load_config(config_path)?;
            config.set_value(&key, &value)?;
            save_config(&config, config_path)?;
            println!("Config updated.");
        }
        ConfigAction::Path => match config_path {
            Some(path) => println!("{}", path.display()),
            None => println!("{}", Config::path().display()),
        },
    }
    Ok(())
}

/// `[REDACTED]` for secret keys with a non-empty value; anything else passes
/// through unchanged.
fn mask_if_secret(key: ConfigKey, value: &str) -> String {
    if key.is_secret() && !value.is_empty() {
        "[REDACTED]".to_string()
    } else {
        value.to_string()
    }
}

/// The config serialized as TOML with every secret token replaced by a
/// placeholder, preserving structure so the output still parses.
fn masked_toml(config: &Config) -> Result<String> {
    let mut masked = config.clone();
    for endpoint in [
        masked.bunkr.as_mut(),
        masked.gofile.as_mut(),
        masked.filester.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        if endpoint.token.is_some() {
            endpoint.token = Some("[REDACTED]".to_string());
        }
    }
    Ok(masked.to_toml()?)
}

fn load_config(config_path: Option<&Path>) -> Result<Config> {
    match config_path {
        Some(path) => Ok(Config::from_file(path)?),
        None => Ok(Config::load()?),
    }
}

fn save_config(config: &Config, config_path: Option<&Path>) -> Result<()> {
    match config_path {
        Some(path) => config.to_file(path)?,
        None => config.save()?,
    }
    Ok(())
}
