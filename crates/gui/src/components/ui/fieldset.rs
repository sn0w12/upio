use dioxus::prelude::*;
use tw_merge::*;

#[component]
pub fn Fieldset(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        form { class: tw_merge!(class), "data-slot": "fieldset", ..attributes, {children} }
    }
}

#[component]
pub fn FieldsetLegend(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("font-semibold text-foreground", class),
            "data-slot": "fieldset-legend",
            ..attributes,
            {children}
        }
    }
}
