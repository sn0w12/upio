use std::collections::HashSet;
use std::time::Duration;

use dioxus::prelude::*;
use upio::registry::UploaderId;
use upio::{PreprocessConfig, UploaderEndpointConfig};
use upio_config::{Config, ConfigKey, Settings};

use crate::components::ui::toast::{use_toast, ToastOptions, Toasts};

/// Transient feedback about the latest write to `config.toml`.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveStatus {
    Idle,
    Saved,
    Error(String),
}

/// Access the shared configuration. Must be called inside
/// [`ConfigProvider`](struct@crate::state::config_store::ConfigProvider).
#[derive(Clone, Copy)]
pub struct ConfigStore {
    config: Signal<Config>,
    env_overridden: Signal<HashSet<&'static str>>,
    pub status: Signal<SaveStatus>,
    /// Set when the config file exists but could not be read/parsed. Writes
    /// are refused while set so a malformed file is never overwritten with
    /// defaults; the Settings dialog shows a blocking reload banner.
    pub load_error: Signal<Option<String>>,
    toast: Toasts,
}

impl ConfigStore {
    /// The current value of a key as stored in the config file.
    pub fn get(&self, key: ConfigKey) -> String {
        self.config.read().get_value(&key)
    }

    /// Whether an environment variable shadows this key; editing it has no
    /// effect until the variable is removed.
    pub fn is_overridden(&self, key: ConfigKey) -> bool {
        self.env_overridden.read().contains(key.as_str())
    }

    /// The endpoint config for a service.
    pub fn endpoint(&self, id: UploaderId) -> UploaderEndpointConfig {
        self.config.read().get_uploader_config(id.name())
    }

    /// The services excluded via `global.disabled_uploaders`.
    pub fn disabled_uploaders(&self) -> HashSet<String> {
        self.config
            .read()
            .global
            .disabled_uploaders
            .iter()
            .cloned()
            .collect()
    }

    /// Enable or disable a service by editing `global.disabled_uploaders`.
    pub fn set_enabled(&mut self, id: UploaderId, enabled: bool) {
        let mut disabled = self.disabled_uploaders();
        if enabled {
            let _ = disabled.remove(id.name());
        } else {
            disabled.insert(id.name().to_string());
        }
        // Stable order matching the registry for readable TOML.
        let names: Vec<&str> = UploaderId::ALL
            .iter()
            .map(|known| known.name())
            .filter(|name| disabled.contains(*name))
            .collect();
        let joined = names
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        self.apply_key(ConfigKey::GlobalDisabledUploaders, joined);
    }

    /// Set a scalar key and persist immediately (instant apply).
    pub fn apply_key(&mut self, key: ConfigKey, value: String) {
        if self.load_error.read().is_some() {
            self.fail("Fix or reload the config file before editing".to_string());
            return;
        }
        let mut next = self.config.read().clone();
        if let Err(e) = next.set_value(&key, &value) {
            self.fail(format!("setting {key}: {e}"));
            return;
        }
        self.persist(next);
    }

    /// Replace the preprocessing rules of one service and persist.
    pub fn apply_preprocess(&mut self, id: UploaderId, preprocess: PreprocessConfig) {
        if self.load_error.read().is_some() {
            self.fail("Fix or reload the config file before editing".to_string());
            return;
        }
        let mut next = self.config.read().clone();
        endpoint_of(&mut next, id).preprocess = preprocess;
        self.persist(next);
    }

    /// Reload from disk (file + environment layers).
    pub fn reload(&mut self) {
        let (file, effective, error) = load_all();
        self.env_overridden
            .set(detect_overridden(&effective, &file));
        self.config.set(file);
        self.load_error.set(error);
        self.status.set(SaveStatus::Idle);
    }

    /// The current config serialized as pretty TOML.
    pub fn raw_toml(&self) -> String {
        self.config.read().to_toml().unwrap_or_default()
    }

    fn persist(&mut self, next: Config) {
        match next.save() {
            Ok(()) => {
                self.config.set(next);
                self.status.set(SaveStatus::Saved);
                let mut status = self.status;
                spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1600)).await;
                    if *status.read() == SaveStatus::Saved {
                        status.set(SaveStatus::Idle);
                    }
                });
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    fn fail(&mut self, message: String) {
        self.status.set(SaveStatus::Error(message.clone()));
        self.toast.error(
            "Couldn't save config".to_string(),
            ToastOptions::new().description(message),
        );
    }
}

/// Access the config store.
pub fn use_config() -> ConfigStore {
    use_context::<ConfigStore>()
}

/// Loads the shared configuration once and provides instant-apply mutation.
///
/// Must be nested inside [`ToastProvider`](crate::components::ui::toast::ToastProvider).
#[component]
pub fn ConfigProvider(children: Element) -> Element {
    let toast = use_toast();

    // The config file is tiny; a blocking load at startup keeps the store
    // simple (no resolve race), matching what the CLI does.
    let (file, effective, load_error) = load_all();
    // Create the file on first launch so it always exists (a missing file is
    // not an error; an unreadable/malformed one is).
    if !Config::path().is_file() {
        let _ = file.save();
    }
    let overridden = detect_overridden(&effective, &file);

    use_context_provider(move || ConfigStore {
        config: Signal::new(file.clone()),
        env_overridden: Signal::new(overridden.clone()),
        status: Signal::new(SaveStatus::Idle),
        load_error: Signal::new(load_error.clone()),
        toast,
    });

    rsx! {
        {children}
    }
}

fn load_all() -> (Config, Config, Option<String>) {
    let file = match Config::load() {
        Ok(file) => file,
        Err(e) => return (Config::default(), Config::default(), Some(e.to_string())),
    };
    let effective = Settings::load()
        .and_then(|settings| settings.extract())
        .unwrap_or_else(|_| file.clone());
    (file, effective, None)
}

fn detect_overridden(effective: &Config, file: &Config) -> HashSet<&'static str> {
    ConfigKey::ALL
        .iter()
        .filter(|key| key.get(effective) != key.get(file))
        .map(|key| key.as_str())
        .collect()
}

fn endpoint_of(config: &mut Config, id: UploaderId) -> &mut UploaderEndpointConfig {
    let section = match id {
        UploaderId::Bunkr => &mut config.bunkr,
        UploaderId::Gofile => &mut config.gofile,
        UploaderId::Fileditch => &mut config.fileditch,
        UploaderId::Filester => &mut config.filester,
        UploaderId::Goonbox => &mut config.goonbox,
    };
    section.get_or_insert_with(Default::default)
}
