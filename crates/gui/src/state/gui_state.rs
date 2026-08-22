use std::path::PathBuf;

use dioxus::prelude::*;

use crate::components::ui::toast::{use_toast, ToastOptions};

use crate::theme::ThemeMode;
use serde::{Deserialize, Serialize};

/// Window geometry persisted across launches.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            maximized: false,
        }
    }
}

/// Remembered upload settings (the Home page controls).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UploadSettings {
    /// Selected service names; unknown names are ignored on load.
    #[serde(default)]
    pub services: Vec<String>,
    /// Max concurrent uploads.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

fn default_batch_size() -> usize {
    4
}

/// GUI-only preferences, stored separately from the shared `config.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GuiState {
    pub theme: ThemeMode,
    #[serde(default)]
    pub window: WindowState,
    #[serde(default)]
    pub upload: UploadSettings,
}

impl GuiState {
    /// The gui state file location, e.g. `%APPDATA%\upio\gui.toml`.
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("upio")
            .join("gui.toml")
    }

    /// Load from disk. Missing file → defaults; malformed or unreadable file
    /// → an error so callers can surface it (never silently reset state).
    pub fn load() -> Result<Self, String> {
        let path = Self::path();
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        toml::from_str(&content).map_err(|e| e.to_string())
    }

    /// Persist atomically: write to a sibling temp file, flush, rename over
    /// the destination so a crash can never truncate `gui.toml`.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        upio_config::write_atomic(&path, content.as_bytes()).map_err(|e| e.to_string())
    }
}

/// The GUI state context provided by [`GuiStateProvider`](struct@crate::state::gui_state::GuiStateProvider).
#[derive(Clone, Copy)]
pub struct GuiStateContext {
    pub state: Signal<GuiState>,
    /// Set when `gui.toml` existed but could not be read/parsed; the app then
    /// renders with defaults instead of silently discarding the user's file.
    pub load_error: Signal<Option<String>>,
}

/// Access the GUI state. Must be called inside [`GuiStateProvider`](struct@crate::state::gui_state::GuiStateProvider).
pub fn use_gui_state() -> GuiStateContext {
    use_context::<GuiStateContext>()
}

/// Loads the GUI state once (falling back to defaults for rendering when the
/// file is malformed) and persists every subsequent change back to
/// `gui.toml`.
#[component]
pub fn GuiStateProvider(children: Element) -> Element {
    let toast = use_toast();

    let (initial, load_error) = match GuiState::load() {
        Ok(state) => (state, None),
        // A missing file is the normal first-launch path; anything else is
        // surfaced to the user.
        Err(_) if !GuiState::path().exists() => (GuiState::default(), None),
        Err(e) => (GuiState::default(), Some(e)),
    };

    let state = use_signal(|| initial);
    let mut load_error = use_signal(move || load_error);
    let mut initialized = use_signal(|| false);

    use_context_provider(|| GuiStateContext { state, load_error });

    // Surface the load error once a toast provider is available.
    use_effect(move || {
        let current_error = load_error.read().clone();
        if let Some(error) = current_error {
            toast.error(
                "Couldn't read gui.toml".to_string(),
                ToastOptions::new().description(format!(
                    "{error} — using default settings until the file is fixed."
                )),
            );
            load_error.set(None);
        }
    });

    // Persist changes and create the file on first launch; save errors are
    // never discarded.
    use_effect(move || {
        let current = state();
        if let Err(e) = current.save() {
            toast.error(
                "Couldn't save settings".to_string(),
                crate::components::ui::toast::ToastOptions::new().description(e),
            );
        }
        if !*initialized.peek() {
            initialized.set(true);
        }
    });

    rsx! {
        {children}
    }
}
