use dioxus::prelude::*;

#[component]
pub fn Form(
    #[props(extends=GlobalAttributes)]
    #[props(extends=button)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        form { "data-slot": "form", ..attributes, {children} }
    }
}
