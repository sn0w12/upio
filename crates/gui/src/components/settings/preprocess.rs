use dioxus::prelude::*;
use tw_merge::*;
use upio::preprocess::{PreprocessConfig, PreprocessRule, PreprocessStrategy, STRATEGIES};
use upio::registry::UploaderId;

use crate::components::ui::field::{Field, FieldDescription, FieldLabel};
use crate::components::ui::label::Label;
use crate::components::ui::select;
use crate::components::ui::switch::Switch;
use crate::state::config_store::{use_config, ConfigStore};
use crate::utils::service_display_name;

use super::rule_editor::StrategyParams;

/// The Preprocessing tab: every strategy is always listed in chain order; a
/// switch decides whether it is configured for the selected service.
#[component]
pub fn PreprocessTab() -> Element {
    let mut store = use_config();
    let mut service = use_signal(|| Some(UploaderId::Bunkr));

    // Both the controlled value and its label derive directly from the
    // owning service signal; no duplicated writable state to go stale.
    let selected_name = use_memo(move || {
        Some(service().unwrap_or(UploaderId::Bunkr).name()) as Option<&'static str>
    });
    let placeholder =
        use_memo(move || service_display_name(service().unwrap_or(UploaderId::Bunkr)).to_string());

    let id = service().unwrap_or(UploaderId::Bunkr);
    let endpoint = store.endpoint(id);
    let preprocess = endpoint.preprocess;

    rsx! {
        div { class: "flex flex-col gap-2",
            div { class: "flex flex-col gap-2 sm:w-56",
                Label { html_for: "preprocess-service", "Service" }
                select::Select::<&'static str> {
                    id: "preprocess-service",
                    value: ReadSignal::new(selected_name),
                    placeholder: ReadSignal::new(placeholder),
                    on_value_change: move |value: Option<&'static str>| {
                        service.set(value.and_then(UploaderId::from_name));
                    },
                    select::SelectGroup {
                        for known in UploaderId::ALL {
                            select::SelectOption::<&'static str> {
                                key: "{known.name()}",
                                index: index_of(known),
                                value: known.name(),
                                text_value: service_display_name(*known),
                                "{service_display_name(*known)}"
                            }
                        }
                    }
                }
            }

            Field { class: "flex-row items-center gap-3",
                Switch {
                    checked: preprocess.enabled,
                    aria_label: "Enable preprocessing",
                    on_checked_change: move |checked: bool| {
                        let mut next = store.endpoint(id).preprocess;
                        next.enabled = checked;
                        store.apply_preprocess(id, next);
                    },
                }
                div { class: "flex flex-col gap-0.5",
                    FieldLabel { "Enable preprocessing" }
                    FieldDescription { "Transform files before they are uploaded to {id.name()}." }
                }
            }

            for strategy in STRATEGIES.iter().copied() {
                StrategyRow {
                    key: "{strategy.def().id}",
                    id,
                    strategy,
                    config: preprocess.clone(),
                }
            }
        }
    }
}

#[component]
fn StrategyRow(id: UploaderId, strategy: PreprocessStrategy, config: PreprocessConfig) -> Element {
    let mut store = use_config();
    let def = strategy.def();
    let is_on = config.rules.iter().any(|rule| rule.strategy == strategy);

    rsx! {
        div {
            class: tw_merge!(
                "rounded-xl border p-4 not-dark:bg-clip-padding", if is_on { "" } else {
                "opacity-64" },
            ),
            div { class: "flex items-start justify-between gap-4",
                div { class: "flex flex-col gap-1",
                    FieldLabel { class: "text-base", "{def.name}" }
                    FieldDescription { "{def.description}" }
                }
                Switch {
                    checked: is_on,
                    aria_label: "Configure {def.name} for {id.name()}",
                    on_checked_change: move |on: bool| toggle_rule(&mut store, id, strategy, on),
                }
            }
            if is_on && def.has_options {
                div { class: "mt-4 border-t pt-4",
                    StrategyParams { service: id, strategy }
                }
            }
        }
    }
}

fn toggle_rule(store: &mut ConfigStore, id: UploaderId, strategy: PreprocessStrategy, on: bool) {
    let mut preprocess = store.endpoint(id).preprocess;
    if on {
        if !preprocess
            .rules
            .iter()
            .any(|rule| rule.strategy == strategy)
        {
            preprocess.rules.push(PreprocessRule::new(strategy));
        }
    } else {
        preprocess.rules.retain(|rule| rule.strategy != strategy);
    }
    store.apply_preprocess(id, preprocess);
}

fn index_of(id: &UploaderId) -> usize {
    UploaderId::ALL
        .iter()
        .position(|known| known == id)
        .unwrap_or(0)
}
