use dioxus::prelude::*;
use upio::preprocess::{PreprocessStrategy, STRATEGIES};
use upio::registry::UploaderId;

use crate::components::ui::field::{Field, FieldDescription, FieldLabel};
use crate::components::ui::input::Input;
use crate::components::ui::select;
use crate::components::ui::switch::Switch;

use crate::state::config_store::{use_config, ConfigStore};

/// Apply a mutation to the configured rule for `strategy` and persist.
fn with_params(
    store: &mut ConfigStore,
    id: UploaderId,
    strategy: PreprocessStrategy,
    f: impl FnOnce(&mut serde_json::Value),
) {
    let mut preprocess = store.endpoint(id).preprocess;
    if let Some(rule) = preprocess
        .rules
        .iter_mut()
        .find(|rule| rule.strategy == strategy)
    {
        f(&mut rule.params);
        store.apply_preprocess(id, preprocess);
    }
}

/// The parameter controls for one strategy's configured rule.
///
/// Renders nothing for strategies without options; must only be mounted when
/// the strategy's rule exists in the config.
#[component]
pub fn StrategyParams(service: UploaderId, strategy: PreprocessStrategy) -> Element {
    let store = use_config();
    let endpoint = store.endpoint(service);
    let Some(params) = endpoint
        .preprocess
        .rules
        .iter()
        .find(|rule| rule.strategy == strategy)
        .map(|rule| rule.params.clone())
    else {
        return rsx! {};
    };

    match strategy.def().id {
        "split_video" => rsx! {},
        "normalize_name" => rsx! {
            div { class: "grid gap-3 sm:grid-cols-3",
                ReplacementParam { service, params: params.clone() }
                MaxLenParam { service, params: params.clone() }
                LowercaseParam { service, params }
            }
        },
        "compress_image" => rsx! {
            div { class: "grid gap-3 sm:grid-cols-3",
                FormatSelect { service, params: params.clone() }
                QualityParam { service, params: params.clone() }
                DimensionsInput { service, params }
            }
        },
        "wrap_zip" => rsx! {
            div { class: "grid gap-3 sm:grid-cols-2",
                LevelParam { service, params: params.clone() }
                StoreUncompressedParam { service, params }
            }
        },
        _ => rsx! {},
    }
}

// Silence the unused-catalog lint when every branch is compiled out by a
// feature flag; STRATEGIES stays the source of truth for ids above.
#[allow(unused)]
fn _catalog_reference() -> &'static [PreprocessStrategy] {
    STRATEGIES
}

#[component]
fn ReplacementParam(service: UploaderId, params: serde_json::Value) -> Element {
    let mut store = use_config();
    let initial = params
        .get("replacement")
        .and_then(|v| v.as_str())
        .unwrap_or("_")
        .to_string();
    let initial_clone = initial.clone();
    let mut local = use_signal(move || initial);

    rsx! {
        Field { class: "gap-1.5",
            FieldLabel { html_for: "param-replacement", "Replacement" }
            Input {
                id: "param-replacement",
                value: "{initial_clone}",
                maxlength: 8,
                autocomplete: "off",
                oninput: move |e: FormEvent| {
                    let value = e.value();
                    local.set(value.clone());
                    with_params(
                        &mut store,
                        service,
                        PreprocessStrategy::by_id("normalize_name").unwrap(),
                        |params| {
                            params["replacement"] = serde_json::Value::String(value.clone());
                        },
                    );
                },
            }
        }
    }
}

#[component]
fn MaxLenParam(service: UploaderId, params: serde_json::Value) -> Element {
    let mut store = use_config();
    let initial = params
        .get("max_len")
        .and_then(|v| v.as_u64())
        .unwrap_or(255);
    let mut invalid = use_signal(|| false);
    let mut local = use_signal(move || initial.to_string());

    rsx! {
        Field { class: "gap-1.5",
            FieldLabel { html_for: "param-max-len", "Max length" }
            Input {
                id: "param-max-len",
                r#type: "number",
                min: 1,
                aria_invalid: *invalid.read(),
                value: "{local}",
                oninput: move |e: FormEvent| {
                    let raw = e.value();
                    local.set(raw.clone());
                    if let Ok(len) = raw.parse::<u64>() {
                        if len > 0 {
                            invalid.set(false);
                            with_params(
                                &mut store,
                                service,
                                PreprocessStrategy::by_id("normalize_name").unwrap(),
                                |params| {
                                    params["max_len"] = serde_json::json!(len);
                                },
                            );
                        } else {
                            invalid.set(true);
                        }
                    } else {
                        invalid.set(true);
                    }
                },
            }
        }
    }
}

#[component]
fn LowercaseParam(service: UploaderId, params: serde_json::Value) -> Element {
    let mut store = use_config();
    let checked = params
        .get("lowercase")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let normalize = PreprocessStrategy::by_id("normalize_name").unwrap();

    rsx! {
        Field { class: "flex-row items-center gap-2 self-end pb-1",
            Switch {
                checked,
                aria_label: "Lowercase",
                on_checked_change: move |checked: bool| {
                    with_params(
                        &mut store,
                        service,
                        normalize,
                        |params| {
                            params["lowercase"] = serde_json::json!(checked);
                        },
                    );
                },
            }
            FieldLabel { "Lowercase" }
        }
    }
}

#[component]
fn FormatSelect(service: UploaderId, params: serde_json::Value) -> Element {
    let mut store = use_config();
    let stored = params
        .get("format")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let static_format = move |format: &Option<String>| match format.as_deref() {
        Some("png") => "png",
        Some("jpeg") | Some("jpg") => "jpeg",
        _ => "auto",
    };
    let current = static_format(&stored);

    let mut selected: Signal<Option<&'static str>> = use_signal(move || Some(current));
    let mut placeholder = use_signal(move || display_format(current).to_string());

    rsx! {
        Field { class: "gap-1.5",
            FieldLabel { html_for: "param-format", "Output format" }
            select::Select::<&'static str> {
                id: "param-format",
                value: ReadSignal::new(selected),
                placeholder: ReadSignal::new(placeholder),
                on_value_change: move |choice: Option<&'static str>| {
                    let choice = choice.unwrap_or("auto");
                    selected.set(Some(choice));
                    placeholder.set(display_format(choice).to_string());
                    with_params(
                        &mut store,
                        service,
                        PreprocessStrategy::by_id("compress_image").unwrap(),
                        |params| {
                            // `null` is not TOML-representable; absent means auto.
                            if choice == "auto" {
                                if let Some(map) = params.as_object_mut() {
                                    map.remove("format");
                                }
                            } else {
                                params["format"] = serde_json::json!(choice);
                            }
                        },
                    );
                },
                select::SelectGroup {
                    select::SelectOption::<&'static str> { index: 0usize, value: "auto", text_value: "Auto", "Auto" }
                    select::SelectOption::<&'static str> { index: 1usize, value: "jpeg", text_value: "JPEG", "JPEG" }
                    select::SelectOption::<&'static str> { index: 2usize, value: "png", text_value: "PNG", "PNG" }
                }
            }
        }
    }
}

fn display_format(format: &'static str) -> &'static str {
    match format {
        "jpeg" => "JPEG",
        "png" => "PNG",
        _ => "Auto",
    }
}

#[component]
fn QualityParam(service: UploaderId, params: serde_json::Value) -> Element {
    let mut store = use_config();
    let initial = params.get("quality").and_then(|v| v.as_u64()).unwrap_or(85);
    let mut invalid = use_signal(|| false);
    let mut local = use_signal(move || initial.to_string());

    rsx! {
        Field { class: "gap-1.5",
            FieldLabel { html_for: "param-quality", "JPEG quality" }
            Input {
                id: "param-quality",
                r#type: "number",
                min: 1,
                max: 100,
                aria_invalid: *invalid.read(),
                value: "{local}",
                oninput: move |e: FormEvent| {
                    let raw = e.value();
                    local.set(raw.clone());
                    match raw.parse::<u64>() {
                        Ok(q) if (1..=100).contains(&q) => {
                            invalid.set(false);
                            with_params(
                                &mut store,
                                service,
                                PreprocessStrategy::by_id("compress_image").unwrap(),
                                |params| {
                                    params["quality"] = serde_json::json!(q);
                                },
                            );
                        }
                        _ => invalid.set(true),
                    }
                },
            }
        }
    }
}

#[component]
fn DimensionsInput(service: UploaderId, params: serde_json::Value) -> Element {
    let mut store = use_config();
    let initial = params
        .get("max_dimensions")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let initial_clone = initial.clone();
    let mut local = use_signal(move || initial);

    rsx! {
        Field { class: "gap-1.5",
            FieldLabel { html_for: "param-dimensions", "Max dimensions" }
            Input {
                id: "param-dimensions",
                value: "{initial_clone}",
                placeholder: "1600x1600",
                autocomplete: "off",
                oninput: move |e: FormEvent| {
                    let raw = e.value();
                    local.set(raw.clone());
                    let parsed = parse_dimensions(&raw);
                    with_params(
                        &mut store,
                        service,
                        PreprocessStrategy::by_id("compress_image").unwrap(),
                        |params| {
                            // `null` is not TOML-representable; absent means no resize.
                            match parsed {
                                Some((width, height)) => {
                                    params["max_dimensions"] =
                                        serde_json::json!(format!("{width}x{height}"));
                                    }
                                None => {
                                    if let Some(map) = params.as_object_mut() {
                                        map.remove("max_dimensions");
                                    }
                                }
                            }
                        },
                    );
                },
            }
            FieldDescription { "Empty disables resizing." }
        }
    }
}

fn parse_dimensions(raw: &str) -> Option<(u32, u32)> {
    let (w, h) = raw.split_once('x')?;
    Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?))
}

#[component]
fn LevelParam(service: UploaderId, params: serde_json::Value) -> Element {
    let mut store = use_config();
    let initial = params.get("level").and_then(|v| v.as_u64()).unwrap_or(6);
    let mut invalid = use_signal(|| false);
    let mut local = use_signal(move || initial.to_string());

    rsx! {
        Field { class: "gap-1.5",
            FieldLabel { html_for: "param-level", "Deflate level" }
            Input {
                id: "param-level",
                r#type: "number",
                min: 0,
                max: 9,
                aria_invalid: *invalid.read(),
                value: "{local}",
                oninput: move |e: FormEvent| {
                    let raw = e.value();
                    local.set(raw.clone());
                    match raw.parse::<u64>() {
                        Ok(level) if level <= 9 => {
                            invalid.set(false);
                            with_params(
                                &mut store,
                                service,
                                PreprocessStrategy::by_id("wrap_zip").unwrap(),
                                |params| {
                                    params["level"] = serde_json::json!(level);
                                },
                            );
                        }
                        _ => invalid.set(true),
                    }
                },
            }
        }
    }
}

#[component]
fn StoreUncompressedParam(service: UploaderId, params: serde_json::Value) -> Element {
    let mut store = use_config();
    let checked = params
        .get("store")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let wrap_zip = PreprocessStrategy::by_id("wrap_zip").unwrap();

    rsx! {
        Field { class: "flex-row items-center gap-2 self-end pb-1",
            Switch {
                checked,
                aria_label: "Store uncompressed",
                on_checked_change: move |checked: bool| {
                    with_params(
                        &mut store,
                        service,
                        wrap_zip,
                        |params| {
                            params["store"] = serde_json::json!(checked);
                        },
                    );
                },
            }
            FieldLabel { "Store uncompressed" }
        }
    }
}
