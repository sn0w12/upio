use dioxus::prelude::*;
use dioxus_primitives::label::{self, LabelProps};
use tw_merge::*;

#[component]
pub fn Label(props: LabelProps) -> Element {
    // The primitive renders `class` then spreads attributes, so a caller
    // class arriving via `attributes` would replace the base — merge it in
    // here instead. (LabelProps carries an optional class-like attribute; we
    // pull it out of `attributes` to be safe.)
    let base =
        "inline-flex items-center gap-2 font-medium text-base/4.5 text-foreground sm:text-sm/4";
    let caller_class = props
        .attributes
        .iter()
        .filter(|a| a.name == "class")
        .cloned()
        .collect::<Vec<_>>();
    let attributes = props
        .attributes
        .iter()
        .filter(|a| a.name != "class")
        .cloned()
        .collect::<Vec<_>>();

    let merged_class = caller_class
        .iter()
        .fold(base.to_string(), |acc, attr| match &attr.value {
            dioxus::core::AttributeValue::Text(text) => tw_merge!(acc, text.to_string()),
            _ => acc,
        });

    rsx! {
        label::Label {
            class: "{merged_class}",
            html_for: props.html_for,
            attributes,
            "data-slot": "label",
            {props.children}
        }
    }
}
