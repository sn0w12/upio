use dioxus::prelude::*;
use tw_merge::*;

/// Used to display a successful result of an action or empty state.
///
/// Ported from coss ui `empty`.
#[component]
pub fn Empty(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "flex min-w-0 flex-1 flex-col items-center justify-center gap-6 text-balance px-6 py-12 text-center md:py-20",
                class,
            ),
            "data-slot": "empty",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn EmptyHeader(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("flex max-w-sm flex-col items-center text-center", class),
            "data-slot": "empty-header",
            ..attributes,
            {children}
        }
    }
}

#[derive(PartialEq, TwVariant)]
pub enum EmptyMediaVariant {
    #[tw(default, class = "bg-transparent")]
    Default,
    #[tw(
        class = "relative flex size-9 shrink-0 items-center justify-center rounded-md border bg-card not-dark:bg-clip-padding text-foreground shadow-sm/5 before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-md)-1px)] before:shadow-[0_1px_--theme(--color-black/4%)] dark:before:shadow-[0_-1px_--theme(--color-white/6%)] [&_svg:not([class*='size-'])]:size-4.5"
    )]
    Icon,
}

#[derive(TwClass)]
#[tw(
    class = "flex shrink-0 items-center justify-center [&_svg]:pointer-events-none [&_svg]:shrink-0"
)]
struct EmptyMediaStyles {
    variant: EmptyMediaVariant,
}

#[component]
pub fn EmptyMedia(
    #[props(default)] variant: EmptyMediaVariant,
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    // Upstream styles each ghost with cn(variants(...), "absolute ..."):
    // twMerge resolves the variant's `relative` against the appended
    // `absolute`, so the ghosts must go through tw_merge too — joining them
    // leaves both classes and `relative` wins in the stylesheet.
    let ghost_base = |extra: &str| {
        tw_merge!(
            EmptyMediaStyles { variant }.to_class(),
            format!("pointer-events-none absolute bottom-px shadow-none {extra}"),
        )
    };
    let wrapper_class = tw_merge!("relative mb-6", class.clone());
    let main_class = tw_merge!(EmptyMediaStyles { variant }.to_class(), class);
    let main_attributes = attributes.clone();

    rsx! {
        div {
            class: wrapper_class,
            "data-slot": "empty-media",
            "data-variant": match variant {
                EmptyMediaVariant::Default => "default",
                EmptyMediaVariant::Icon => "icon",
            },
            ..attributes,
            if variant == EmptyMediaVariant::Icon {
                div {
                    class: ghost_base("origin-bottom-left -translate-x-0.5 -rotate-10 scale-84"),
                    aria_hidden: "true",
                }
                div {
                    class: ghost_base("origin-bottom-right translate-x-0.5 rotate-10 scale-84"),
                    aria_hidden: "true",
                }
            }
            div { class: main_class, ..main_attributes, {children} }
        }
    }
}

#[component]
pub fn EmptyTitle(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("font-heading font-semibold text-xl", class),
            "data-slot": "empty-title",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn EmptyDescription(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "text-muted-foreground text-sm [&>a:hover]:text-primary [&>a]:underline [&>a]:underline-offset-4 [[data-slot=empty-title]+&]:mt-1",
                class,
            ),
            "data-slot": "empty-description",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn EmptyContent(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "flex w-full min-w-0 max-w-sm flex-col items-center gap-4 text-balance text-sm",
                class,
            ),
            "data-slot": "empty-content",
            ..attributes,
            {children}
        }
    }
}
