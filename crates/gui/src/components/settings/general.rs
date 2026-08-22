use dioxus::prelude::*;
use dioxus_icons::lucide::{Copy, ExternalLink};

use crate::components::ui::button::{button_class, ButtonSize, ButtonVariant};
use crate::components::ui::fieldset::{Fieldset, FieldsetLegend};
use crate::components::ui::input_group::{
    InputGroup, InputGroupAddon, InputGroupAddonAlign, InputGroupInput,
};
use crate::components::ui::label::Label;
use crate::components::ui::select;
use crate::components::ui::tooltip::{Tooltip, TooltipContent, TooltipTrigger};
use crate::state::gui_state::GuiState;
use crate::theme::{use_theme, ThemeMode};
use crate::utils::{copy_text, reveal_path};
use upio_config::Config;

#[component]
pub fn GeneralTab() -> Element {
    rsx! {
        div { class: "flex flex-col gap-4",
            AppearanceSection {}
            FileLocationsSection {}
        }
    }
}

#[component]
fn AppearanceSection() -> Element {
    let mut theme = use_theme();

    // Both the controlled value and its label derive directly from the
    // owning ThemeContext signal; no duplicated writable state to go stale.
    let selected = use_memo(move || Some((theme.mode)().as_str()) as Option<&'static str>);
    let placeholder = use_memo(move || ThemeMode::label((theme.mode)()).to_string());

    rsx! {
        Fieldset { class: "flex flex-col gap-2",
            FieldsetLegend { "Appearance" }
            div { class: "flex flex-col gap-2",
                Label { html_for: "setting-theme", "Theme" }
                select::Select::<&'static str> {
                    id: "setting-theme",
                    value: ReadSignal::new(selected),
                    placeholder: ReadSignal::new(placeholder),
                    on_value_change: move |value: Option<&'static str>| {
                        if let Some(value) = value {
                            theme.mode.set(ThemeMode::parse(value));
                        }
                    },
                    select::SelectGroup {
                        select::SelectOption::<&'static str> {
                            index: 0usize,
                            value: "system",
                            text_value: "System",
                            "System"
                        }
                        select::SelectOption::<&'static str> {
                            index: 1usize,
                            value: "light",
                            text_value: "Light",
                            "Light"
                        }
                        select::SelectOption::<&'static str> {
                            index: 2usize,
                            value: "dark",
                            text_value: "Dark",
                            "Dark"
                        }
                    }
                }
                p { class: "text-muted-foreground text-xs",
                    "\"System\" follows your operating system setting."
                }
            }
        }
    }
}

#[component]
fn FileLocationsSection() -> Element {
    let config_path = Config::path();
    let gui_path = GuiState::path();

    rsx! {
        Fieldset { class: "flex flex-col gap-2",
            FieldsetLegend { "File locations" }
            div { class: "flex flex-col gap-3",
                PathRow {
                    label: "Config file",
                    description: "Shared with the upio CLI. Services and preprocessing rules.",
                    path: config_path,
                }
                PathRow {
                    label: "GUI state",
                    description: "GUI-only preferences such as the theme and window size.",
                    path: gui_path,
                }
            }
        }
    }
}

#[component]
fn PathRow(label: String, description: String, path: std::path::PathBuf) -> Element {
    let path_text = path.display().to_string();
    let input_id = match label.as_str() {
        "Config file" => "config-path",
        _ => "gui-state-path",
    };

    rsx! {
        div { class: "flex flex-col gap-1.5",
            Label { html_for: "{input_id}", "{label}" }
            InputGroup {
                InputGroupInput {
                    id: input_id,
                    r#type: "text",
                    value: "{path_text}",
                    readonly: true,
                    class: "font-mono text-xs",
                    aria_label: "{label} path",
                }
                InputGroupAddon { align: InputGroupAddonAlign::InlineEnd,
                    IconAction {
                        icon: "copy",
                        tooltip_text: "Copy path",
                        aria_label: "Copy {label} path",
                        onclick: move |_| copy_text(&path_text),
                    }
                    IconAction {
                        icon: "open",
                        tooltip_text: "Reveal in file manager",
                        aria_label: "Reveal {label} path in file manager",
                        onclick: move |_| reveal_path(&path),
                    }
                }
            }
            p { class: "text-muted-foreground text-xs", "{description}" }
        }
    }
}

/// A small ghost icon button with a tooltip, used inside input groups.
///
#[component]
fn IconAction(
    icon: &'static str,
    tooltip_text: &'static str,
    aria_label: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let trigger = move |attributes: Vec<Attribute>| {
        rsx! {
            button {
                r#type: "button",
                class: button_class(ButtonSize::IconSm, ButtonVariant::Ghost),
                aria_label: "{aria_label}",
                onclick: move |e| onclick.call(e),
                ..attributes,
                if icon == "copy" {
                    Copy {}
                } else {
                    ExternalLink {}
                }
            }
        }
    };

    rsx! {
        Tooltip {
            TooltipTrigger { r#as: trigger }
            TooltipContent { "{tooltip_text}" }
        }
    }
}
