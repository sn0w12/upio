//! The all-in-one settings dialog. Every configurable value of upio lives in
//! one of its tabs and applies instantly to `config.toml`.

mod config_file;
mod general;
mod preprocess;
mod rule_editor;
mod services;
mod tools;

use dioxus::prelude::*;
use dioxus_icons::lucide::{Check, CircleAlert};

use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::dialog::{
    Dialog, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
};
use crate::components::ui::tabs::{TabContent, TabList, TabTrigger, Tabs, TabsVariant};
use crate::state::config_store::{use_config, SaveStatus};
use crate::state::gui_state::use_gui_state;

/// Whether the settings dialog is currently open.
pub static SETTINGS_OPEN: GlobalSignal<bool> = Signal::global(|| false);

/// Open or close the settings dialog from anywhere.
pub fn set_settings_open(value: bool) {
    SETTINGS_OPEN.signal().set(value);
}

const TAB_GENERAL: &str = "general";
const TAB_SERVICES: &str = "services";
const TAB_PREPROCESS: &str = "preprocess";
const TAB_TOOLS: &str = "tools";
const TAB_CONFIG_FILE: &str = "config-file";

#[component]
pub fn SettingsDialog() -> Element {
    let mut open_value = use_signal(|| Some(false));
    let mut active_tab = use_signal(|| None::<String>);
    let mut store = use_config();
    let load_error = store.load_error.read().clone();
    let gui_state_error = use_gui_state().load_error.read().clone();

    // Keep the local dialog state in sync with the global flag.
    use_effect(move || {
        open_value.set(Some(*SETTINGS_OPEN.read()));
    });

    rsx! {
        Dialog {
            open: open_value,
            class: "max-w-4xl h-[calc(100vh-12rem)]",
            on_open_change: move |open: bool| {
                set_settings_open(open);
                open_value.set(Some(open));
            },
            DialogHeader { class: "shrink-0",
                DialogTitle { "Settings" }
                DialogDescription { "Changes are applied to the config file as you make them." }
            }
            if let Some(error) = load_error {
                div { class: "mx-6 flex shrink-0 flex-col gap-2 rounded-lg border border-destructive/32 bg-destructive/4 px-3.5 py-2.5 text-sm",
                    div { class: "flex items-center gap-2 font-medium text-destructive",
                        CircleAlert { class: "size-4" }
                        "The config file could not be loaded"
                    }
                    p { class: "font-mono text-muted-foreground text-xs", "{error}" }
                    p { class: "text-muted-foreground text-xs",
                        "Editing is disabled so your file is not overwritten with defaults. "
                        "Fix the file, then reload."
                    }
                    Button {
                        class: "self-start",
                        size: crate::components::ui::button::ButtonSize::Sm,
                        variant: ButtonVariant::Outline,
                        onclick: move |_| store.reload(),
                        "Reload config"
                    }
                }
            }
            if let Some(error) = gui_state_error {
                div { class: "mx-6 flex shrink-0 flex-col gap-1 rounded-lg border border-destructive/32 bg-destructive/4 px-3.5 py-2.5 text-sm",
                    div { class: "flex items-center gap-2 font-medium text-destructive",
                        CircleAlert { class: "size-4" }
                        "The GUI state file (gui.toml) could not be loaded"
                    }
                    p { class: "font-mono text-muted-foreground text-xs", "{error}" }
                }
            }
            Tabs {
                value: active_tab,
                default_value: TAB_GENERAL.to_string(),
                on_value_change: move |value: String| active_tab.set(Some(value)),
                class: "min-h-0 flex-1 flex flex-col",
                TabList {
                    variant: TabsVariant::Underlined,
                    class: "mx-6 shrink-0 justify-start",
                    TabTrigger {
                        index: 0usize,
                        value: TAB_GENERAL,
                        variant: TabsVariant::Underlined,
                        "General"
                    }
                    TabTrigger {
                        index: 1usize,
                        value: TAB_SERVICES,
                        variant: TabsVariant::Underlined,
                        "Services"
                    }
                    TabTrigger {
                        index: 2usize,
                        value: TAB_PREPROCESS,
                        variant: TabsVariant::Underlined,
                        "Preprocessing"
                    }
                    TabTrigger {
                        index: 3usize,
                        value: TAB_TOOLS,
                        variant: TabsVariant::Underlined,
                        "Tools"
                    }
                    TabTrigger {
                        index: 4usize,
                        value: TAB_CONFIG_FILE,
                        variant: TabsVariant::Underlined,
                        "Config file"
                    }
                }
                div { class: "min-h-0 flex-1 overflow-y-auto px-6 py-2",
                    TabContent { index: 0usize, value: TAB_GENERAL, general::GeneralTab {} }
                    TabContent { index: 1usize, value: TAB_SERVICES, services::ServicesTab {} }
                    TabContent { index: 2usize, value: TAB_PREPROCESS, preprocess::PreprocessTab {} }
                    TabContent { index: 3usize, value: TAB_TOOLS, tools::ToolsTab {} }
                    TabContent { index: 4usize, value: TAB_CONFIG_FILE, config_file::ConfigFileTab {} }
                }
            }
            DialogFooter { class: "shrink-0 items-center sm:justify-between",
                SaveIndicator {}
                Button {
                    variant: ButtonVariant::Outline,
                    onclick: move |_| set_settings_open(false),
                    "Close"
                }
            }
        }
    }
}

#[component]
fn SaveIndicator() -> Element {
    let store = use_config();

    rsx! {
        div { class: "flex items-center gap-2 text-sm text-muted-foreground",
            match &*store.status.read() {
                SaveStatus::Idle => rsx! {},
                SaveStatus::Saved => rsx! {
                    Check { class: "size-4 text-success" }
                    "Saved"
                },
                SaveStatus::Error(message) => rsx! {
                    CircleAlert { class: "size-4 text-destructive" }
                    "{message}"
                },
            }
        }
    }
}
