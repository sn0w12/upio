use dioxus::prelude::*;
use tw_merge::*;

/// Displays a keyboard key or shortcut.
///
/// Ported from coss ui `kbd`.
#[component]
pub fn Kbd(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        kbd {
            class: tw_merge!(
                "pointer-events-none inline-flex h-5 min-w-5 select-none items-center justify-center gap-1 rounded-[.25rem] bg-muted px-1 font-medium font-sans text-muted-foreground text-xs [&_svg:not([class*='size-'])]:size-3",
                class,
            ),
            "data-slot": "kbd",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn KbdGroup(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        kbd {
            class: tw_merge!("inline-flex items-center gap-1", class),
            "data-slot": "kbd-group",
            ..attributes,
            {children}
        }
    }
}
