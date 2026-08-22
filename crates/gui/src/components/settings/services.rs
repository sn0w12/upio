use std::time::Duration;

use dioxus::prelude::*;
use dioxus_icons::lucide::{Eye, EyeOff};
use tw_merge::*;
use upio::registry::UploaderId;
use upio_config::ConfigKey;

use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::card::{Card, CardContent};
use crate::components::ui::field::{Field, FieldDescription, FieldLabel};
use crate::components::ui::input::Input;
use crate::components::ui::switch::Switch;
use crate::components::ui::tooltip::{Tooltip, TooltipContent, TooltipTrigger};
use crate::state::config_store::use_config;
use crate::utils::format_size;

/// The Services tab: enable services and edit their endpoint settings.
#[component]
pub fn ServicesTab() -> Element {
    let selected = use_signal(|| Some(UploaderId::Bunkr));

    rsx! {
        div { class: "flex gap-6 flex-row",
            ServiceList { selected }
            if let Some(id) = selected() {
                ServiceDetail { key: "{id.name()}", id }
            }
        }
    }
}

#[component]
fn ServiceList(selected: Signal<Option<UploaderId>>) -> Element {
    rsx! {
        Card { aria_label: "Services", class: "h-fit",
            CardContent { class: "p-1",
                for id in UploaderId::ALL {
                    ServiceRow { key: "{id.name()}", id: *id, selected }
                }
            }
        }
    }
}

#[component]
fn ServiceRow(id: UploaderId, selected: Signal<Option<UploaderId>>) -> Element {
    let store = use_config();
    let is_selected = selected() == Some(id);
    let is_enabled = !store.disabled_uploaders().contains(id.name());

    rsx! {
        button {
            class: tw_join!(
                "flex w-full items-center justify-between gap-2 rounded-xl px-2.5 py-1.5 text-sm transition-colors",
                if is_selected { "bg-accent text-accent-foreground" } else {
                "text-muted-foreground hover:bg-accent/50 hover:text-foreground" },
            ),
            aria_pressed: if is_selected { "true" } else { "false" },
            onclick: move |_| selected.set(Some(id)),
            span { class: if is_enabled { "font-medium text-foreground" } else { "" }, "{id.name()}" }
            span {
                class: tw_join!(
                    "size-1.5 shrink-0 rounded-full", if is_enabled { "bg-success" } else {
                    "bg-muted-foreground/40" },
                ),
            }
        }
    }
}

#[component]
fn ServiceDetail(id: UploaderId) -> Element {
    let store = use_config();
    let disabled = store.disabled_uploaders();
    let enabled = !disabled.contains(id.name());
    let capabilities = id.capabilities();

    rsx! {
        div { class: "flex flex-1 min-w-0 flex-col gap-3",
            div { class: "flex flex-col items-start gap-1",
                h3 { class: "font-heading font-semibold text-lg capitalize", "{id.name()}" }
                div { class: "flex gap-1 flex-wrap",
                    for name in capabilities.names() {
                        Badge { variant: BadgeVariant::Secondary, "{name}" }
                    }
                    Badge { variant: BadgeVariant::Outline, "{format_size(id.max_file_size())}" }
                }
            }

            EnabledRow { id, enabled }

            match id {
                UploaderId::Fileditch => rsx! {
                    p { class: "text-sm text-muted-foreground", "Fileditch needs no configuration and accepts any file." }
                },
                _ => rsx! {
                    div { class: "flex flex-col gap-5",
                        ConfigInput {
                            config_key: token_key(id).expect("token field only for tokened services"),
                            label: "Token",
                            description: token_description(id),
                            placeholder: token_placeholder(id),
                            password: true,
                        }
                        if id == UploaderId::Gofile {
                            ConfigInput {
                                config_key: ConfigKey::GofileServer,
                                label: "Server",
                                description: "Pin a specific server (e.g. \"store4\"), or leave empty to auto-select.",
                                placeholder: "auto",
                                password: false,
                            }
                        }
                    }
                },
            }
        }
    }
}

fn token_key(id: UploaderId) -> Option<ConfigKey> {
    match id {
        UploaderId::Bunkr => Some(ConfigKey::BunkrToken),
        UploaderId::Gofile => Some(ConfigKey::GofileToken),
        UploaderId::Filester => Some(ConfigKey::FilesterToken),
        // Fileditch needs no token; the GUI renders no field for it.
        UploaderId::Fileditch => None,
    }
}

fn token_description(id: UploaderId) -> &'static str {
    match id {
        UploaderId::Bunkr => "Required. Create one in your Bunkr account settings.",
        UploaderId::Gofile => "Optional. Raises the size limit and unlocks folders.",
        UploaderId::Filester => "Optional.",
        UploaderId::Fileditch => "",
    }
}

fn token_placeholder(id: UploaderId) -> &'static str {
    match id {
        UploaderId::Bunkr => "Paste your Bunkr token",
        UploaderId::Gofile => "Paste your GoFile token",
        UploaderId::Filester => "Paste your Filester token",
        UploaderId::Fileditch => "",
    }
}

#[component]
fn EnabledRow(id: UploaderId, enabled: bool) -> Element {
    let mut store = use_config();

    rsx! {
        Field { class: "flex-row items-center gap-3",
            Switch {
                checked: enabled,
                on_checked_change: move |checked: bool| store.set_enabled(id, checked),
                aria_label: "Enable {id.name()}",
            }
            div { class: "flex flex-col gap-0.5",
                FieldLabel { "Enabled" }
                FieldDescription { "Disabled services are hidden from the upload picker." }
            }
        }
    }
}

/// A labeled input bound to a [`ConfigKey`] with instant apply.
///
/// Applies 600 ms after the last keystroke, immediately on Enter, and when
/// the field loses focus. Empty clears the value.
#[component]
fn ConfigInput(
    config_key: ConfigKey,
    label: String,
    description: String,
    placeholder: &'static str,
    password: bool,
) -> Element {
    let mut store = use_config();
    let mut local = use_signal(move || store.get(config_key));

    // Re-sync the text field whenever the backing config value changes for
    // any reason (reload, env override, another editor of the same key).
    use_effect(move || {
        let backing = store.get(config_key);
        if local.peek().as_str() != backing {
            local.set(backing);
        }
    });

    let mut generation = use_signal(|| 0u64);
    let mut revealed = use_signal(|| false);

    // Debounced write-through: applies after the user stops typing.
    use_effect(move || {
        let value = local();
        *generation.write() += 1;
        let my_generation = *generation.peek();
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            if generation() == my_generation && store.get(config_key) != value {
                store.apply_key(config_key, value);
            }
        });
    });

    let commit_on_enter = move |e: KeyboardEvent| {
        if e.key() == Key::Enter {
            e.prevent_default();
            *generation.write() += 1;
            let value = local.peek().clone();
            store.apply_key(config_key, value);
        }
    };
    let commit_on_blur = move |_| {
        // Apply any uncommitted text right away.
        *generation.write() += 1;
        let value = local.peek().clone();
        store.apply_key(config_key, value);
    };

    let overridden = store.is_overridden(config_key);
    let env_name = format!(
        "UPIO_{}",
        config_key.as_str().to_uppercase().replace('.', "_")
    );

    rsx! {
        Field { class: "gap-1.5",
            div { class: "flex items-center gap-2",
                FieldLabel { html_for: "config-{config_key}", "{label}" }
                if overridden {
                    Tooltip {
                        TooltipTrigger {
                            span {
                                Badge { variant: BadgeVariant::Info, "env" }
                            }
                        }
                        TooltipContent {
                            "Set by the {env_name} environment variable, which takes "
                            "precedence over the config file."
                        }
                    }
                }
            }
            div { class: "relative w-full",
                Input {
                    id: "config-{config_key}",
                    r#type: if password && !revealed() { "password" } else { "text" },
                    value: "{local}",
                    disabled: overridden,
                    placeholder,
                    autocomplete: "off",
                    class: if password { "pr-10" } else { "" },
                    oninput: move |e: FormEvent| local.set(e.value()),
                    onkeydown: commit_on_enter,
                    onblur: commit_on_blur,
                }
                if password {
                    button {
                        class: "absolute inset-y-0 right-0 flex w-9 items-center justify-center text-muted-foreground hover:text-foreground",
                        r#type: "button",
                        aria_label: if revealed() { "Hide value" } else { "Show value" },
                        onclick: move |_| revealed.toggle(),
                        if revealed() {
                            EyeOff { class: "size-4" }
                        } else {
                            Eye { class: "size-4" }
                        }
                    }
                }
            }
            FieldDescription { "{description}" }
        }
    }
}
