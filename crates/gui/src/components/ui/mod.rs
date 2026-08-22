//! Shared helpers for the UI kit ports.

pub mod badge;
pub mod button;
pub mod card;
pub mod copy_button;
pub mod dialog;
pub mod empty;
pub mod field;
pub mod fieldset;
pub mod form;
pub mod input;
pub mod input_group;
pub mod kbd;
pub mod label;
pub mod progress;
pub mod select;
pub mod separator;
pub mod spinner;
pub mod switch;
pub mod table;
pub mod tabs;
pub mod textarea;
pub mod toast;
pub mod tooltip;

use dioxus::prelude::*;

/// Split an attribute list into its `class` value (if any) and the rest.
///
/// Ports whose underlying primitive spreads `attributes` after its own
/// classes use this so a caller-supplied class merges instead of overriding.
pub(crate) fn split_class(attributes: Vec<Attribute>) -> (Option<String>, Vec<Attribute>) {
    let mut class = None;
    let rest = attributes
        .into_iter()
        .filter(|attr| {
            if attr.name == "class" {
                if let dioxus::core::AttributeValue::Text(text) = &attr.value {
                    class = Some(text.to_string());
                }
                false
            } else {
                true
            }
        })
        .collect();
    (class, rest)
}
