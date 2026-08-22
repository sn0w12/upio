use crate::components::settings::set_settings_open;
use crate::state::gui_state::use_gui_state;
use crate::theme::use_theme;
use crate::Route;
use dioxus::desktop::tao::window::Theme;
use dioxus::prelude::*;

/// Global keyboard shortcut: Ctrl+, opens settings.
///
/// The JS listener is registered under a named cleanup handle so a remount
/// replaces it instead of stacking duplicates; `use_drop` removes it when the
/// component unmounts, and the receive task dies with the scope.
fn use_open_settings_shortcut() {
    dioxus::core::use_drop(|| {
        _ = document::eval(
            "if (window.__upioSettingsShortcutCleanup) { \
                 window.__upioSettingsShortcutCleanup(); \
                 delete window.__upioSettingsShortcutCleanup; \
             }",
        );
    });

    use_effect(move || {
        let mut eval = document::eval(
            r#"
            if (window.__upioSettingsShortcutCleanup) {
                window.__upioSettingsShortcutCleanup();
            }
            const handler = (event) => {
                if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key === ',') {
                    event.preventDefault();
                    dioxus.send(true);
                }
            };
            window.addEventListener('keydown', handler);
            window.__upioSettingsShortcutCleanup = () => window.removeEventListener('keydown', handler);
            "#,
        );
        spawn(async move {
            while let Ok(true) = eval.recv::<bool>().await {
                set_settings_open(true);
            }
        });
    });
}

/// Persist window size and maximize state. The webview reports resizes
/// (debounced in JS so dragging the border does not write to disk on every
/// pixel); every resize is also a chance to re-read the OS maximize state,
/// which catches un-maximizing done outside our controls.
///
/// The resize listener is registered under a named cleanup handle and removed
/// via `use_drop` on unmount so remounts never stack listeners or tasks.
fn use_window_persistence() {
    let gui = use_gui_state();

    dioxus::core::use_drop(|| {
        _ = document::eval(
            "if (window.__upioResizeCleanup) { \
                 window.__upioResizeCleanup(); \
                 delete window.__upioResizeCleanup; \
             }",
        );
    });

    let window_handle = dioxus::desktop::use_window();
    use_effect(move || {
        let mut eval = document::eval(
            r#"
            if (window.__upioResizeCleanup) {
                window.__upioResizeCleanup();
            }
            let timer = null;
            const report = () => dioxus.send({
                width: window.innerWidth,
                height: window.innerHeight
            });
            const handler = () => {
                clearTimeout(timer);
                timer = setTimeout(report, 300);
            };
            window.addEventListener('resize', handler);
            window.__upioResizeCleanup = () => {
                clearTimeout(timer);
                window.removeEventListener('resize', handler);
            };
            report();
            "#,
        );
        let mut state = gui.state;
        let window = window_handle.clone();
        spawn(async move {
            while let Ok(size) = eval.recv::<serde_json::Value>().await {
                let width = size["width"].as_f64().unwrap_or(0.0);
                let height = size["height"].as_f64().unwrap_or(0.0);
                if width > 0.0 && height > 0.0 {
                    let is_maximized = window.is_maximized();
                    // Mutate through the guard: `window` is Copy.
                    let mut persisted = state.write();
                    // Never persist the maximized dimensions; keep the last
                    // windowed geometry so restoring looks right.
                    if !is_maximized {
                        persisted.window.width = width;
                        persisted.window.height = height;
                    }
                    persisted.window.maximized = is_maximized;
                }
            }
        });
    });
}

#[component]
pub fn Layout() -> Element {
    let theme = use_theme();
    let window = dioxus::desktop::use_window();

    // Keep the native titlebar in sync with the resolved theme: dark/light
    // via tao, plus sidebar-tinted caption colors and a hidden caption
    // icon/title on Windows.
    use_effect(move || {
        let dark = (theme.is_dark)();
        window.set_theme(Some(if dark { Theme::Dark } else { Theme::Light }));

        #[cfg(windows)]
        {
            use dioxus::desktop::tao::platform::windows::WindowExtWindows;

            // tao's HWND and windows-sys' are both `*mut c_void`.
            let hwnd = window.hwnd() as *mut core::ffi::c_void;
            crate::platform::apply_caption_colors(hwnd, crate::platform::sidebar_rgb(dark));
            crate::platform::hide_caption_icon(hwnd);
        }
    });

    use_open_settings_shortcut();
    use_window_persistence();

    rsx! {
        div { class: "flex h-screen flex-col overflow-hidden",
            div { class: "min-h-0 flex-1 overflow-auto", Outlet::<Route> {} }
        }
    }
}
