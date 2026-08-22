use dioxus::prelude::*;
use tw_merge::*;

#[component]
pub fn Field(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("flex flex-col items-start gap-2", class),
            "data-slot": "field",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn FieldLabel(
    #[props(default)] html_for: Option<String>,
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let for_attr = html_for
        .map(|value| dioxus_primitives::dioxus_attributes::attributes!(label { r#for: "{value}" }));
    rsx! {
        label {
            class: tw_merge!(
                "inline-flex items-center gap-2 font-medium text-base/4.5 text-foreground data-disabled:opacity-64 sm:text-sm/4",
                class,
            ),
            "data-slot": "field-label",
            ..for_attr.unwrap_or_default(),
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn FieldDescription(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        p {
            class: tw_merge!("text-muted-foreground text-xs", class),
            "data-slot": "field-description",
            ..attributes,
            {children}
        }
    }
}
