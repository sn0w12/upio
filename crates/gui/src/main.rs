use crate::components::settings::SettingsDialog;
use crate::components::ui::toast::ToastProvider;
use crate::state::config_store::ConfigProvider;
use crate::state::gui_state::{GuiState, GuiStateProvider};
use crate::state::upload_queue::UploadQueueProvider;
use crate::theme::ThemeProvider;
use dioxus::{
    desktop::{Config, LogicalSize, WindowBuilder},
    prelude::*,
};
use views::{Home, Layout};

mod components;
#[cfg(windows)]
mod platform;
mod state;
mod theme;
mod utils;
mod views;

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Layout)]
    #[route("/")]
    Home {},
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const GEIST_SANS: Asset = asset!("/assets/fonts/GeistVariable.woff2");
const GEIST_MONO: Asset = asset!("/assets/fonts/GeistMonoVariable.woff2");

fn main() {
    // Bootstrap the window from the last persisted geometry when it can be
    // read; a malformed file must not block startup (the provider surfaces
    // the error in-app).
    let window = GuiState::load().unwrap_or_default().window;
    dioxus::LaunchBuilder::new()
        .with_cfg(desktop! {
            Config::new()
                .with_disable_context_menu(false)
                .with_menu(None)
                .with_window(
                WindowBuilder::new()
                    .with_title("Upio")
                    .with_inner_size(LogicalSize::new(window.width, window.height))
                    .with_maximized(window.maximized)
                    .with_decorations(true),
            )
        })
        .launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        style { "{font_faces()}" }
        ToastProvider {
            GuiStateProvider {
                ThemeProvider {
                    ConfigProvider {
                        UploadQueueProvider {
                            Router::<Route> {}
                            SettingsDialog {}
                        }
                    }
                }
            }
        }
    }
}

fn font_faces() -> String {
    format!(
        "@font-face {{ font-family: \"Geist Variable\"; font-style: normal; font-weight: 100 900; font-display: swap; src: url(\"{GEIST_SANS}\") format(\"woff2\"); }} \
         @font-face {{ font-family: \"Geist Mono Variable\"; font-style: normal; font-weight: 100 900; font-display: swap; src: url(\"{GEIST_MONO}\") format(\"woff2\"); }}",
    )
}
