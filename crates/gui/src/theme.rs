use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::state::gui_state::use_gui_state;

/// The user's theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Follow the OS setting.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    /// The canonical lowercase name, as stored in `gui.toml`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    /// The display label.
    pub const fn label(self) -> &'static str {
        match self {
            ThemeMode::System => "System",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }

    /// Parse a mode from its canonical name.
    pub fn parse(value: &str) -> Self {
        match value {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

/// The theme context provided by [`ThemeProvider`].
#[derive(Clone, Copy)]
pub struct ThemeContext {
    /// The user's theme preference.
    pub mode: Signal<ThemeMode>,
    /// The resolved (effective) dark state, after applying the preference to
    /// the system setting.
    pub is_dark: Signal<bool>,
}

/// Access the current theme preference and resolved dark state.
///
/// Must be called inside a [`ThemeProvider`].
pub fn use_theme() -> ThemeContext {
    use_context::<ThemeContext>()
}

/// Provides the theme to the app and keeps the `dark` class on `<html>`
/// in sync with the resolved (light/dark) theme.
#[component]
pub fn ThemeProvider(children: Element) -> Element {
    let mut gui = use_gui_state();
    let mode = use_signal(|| gui.state.peek().theme);
    let system_dark = use_signal(|| false);
    let mut is_dark = use_signal(|| false);

    // Persist preference changes back into the GUI state file.
    use_effect(move || {
        let current = mode();
        if gui.state.peek().theme != current {
            gui.state.write().theme = current;
        }
    });

    // Subscribe to the OS color-scheme and keep `system_dark` updated. The
    // JS media-query listener is registered under a named cleanup handle so
    // remounts replace it instead of stacking; `use_drop` unregisters it.
    dioxus::core::use_drop(|| {
        _ = document::eval(
            "if (window.__upioColorSchemeCleanup) { \
                 window.__upioColorSchemeCleanup(); \
                 delete window.__upioColorSchemeCleanup; \
             }",
        );
    });

    use_effect(move || {
        let mut eval = document::eval(
            r#"
            if (window.__upioColorSchemeCleanup) {
                window.__upioColorSchemeCleanup();
            }
            const mq = window.matchMedia('(prefers-color-scheme: dark)');
            const handler = (e) => dioxus.send(e.matches);
            mq.addEventListener('change', handler);
            window.__upioColorSchemeCleanup = () => mq.removeEventListener('change', handler);
            dioxus.send(mq.matches);
            "#,
        );
        let mut system_dark = system_dark;
        spawn(async move {
            while let Ok(matches) = eval.recv::<bool>().await {
                system_dark.set(matches);
            }
        });
    });

    // Resolve the effective dark state from the preference + system setting
    // and apply the `dark` class to the document root.
    use_effect(move || {
        let dark = match mode() {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => system_dark(),
        };
        is_dark.set(dark);
        _ = document::eval(&format!(
            "document.documentElement.classList.toggle('dark', {dark});"
        ));
    });

    use_context_provider(|| ThemeContext { mode, is_dark });

    rsx! {
        {children}
    }
}
