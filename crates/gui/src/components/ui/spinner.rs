use dioxus::prelude::*;
use dioxus_icons::lucide::LoaderCircle;
use tw_merge::*;

/// A loading indicator.
///
/// Ported from coss ui `spinner`.
#[component]
pub fn Spinner(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    rsx! {
        LoaderCircle {
            class: tw_merge!("animate-spin", class),
            role: "status",
            "aria-label": "Loading",
            attributes,
        }
    }
}
