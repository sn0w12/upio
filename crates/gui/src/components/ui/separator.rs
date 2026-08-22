use dioxus::prelude::*;
use dioxus_primitives::separator::{self, SeparatorProps};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

/// Separates content sections.
///
/// Ported from coss ui `separator`.
#[component]
pub fn Separator(props: SeparatorProps) -> Element {
    let base = attributes!(div {
        class: "shrink-0 bg-border data-[orientation=horizontal]:h-px data-[orientation=horizontal]:w-full data-[orientation=vertical]:w-px data-[orientation=vertical]:not-[[class^='h-']]:not-[[class*='_h-']]:self-stretch",
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        separator::Separator {
            horizontal: props.horizontal,
            decorative: props.decorative,
            attributes: merged,
            "data-slot": "separator",
            {props.children}
        }
    }
}
