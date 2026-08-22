use dioxus::prelude::*;
use tw_merge::*;

#[component]
pub fn Card(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "relative flex flex-col rounded-2xl border bg-card not-dark:bg-clip-padding text-card-foreground shadow-xs/5 before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-2xl)-1px)] before:shadow-[0_1px_--theme(--color-black/4%)] dark:before:shadow-[0_-1px_--theme(--color-white/6%)]",
                class,
            ),
            "data-slot": "card",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardHeader(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "grid auto-rows-min grid-rows-[auto_auto] items-start gap-1.5 p-6 in-[[data-slot=card]:has(>[data-slot=card-panel])]:pb-4 has-data-[slot=card-action]:grid-cols-[1fr_auto]",
                class,
            ),
            "data-slot": "card-header",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardTitle(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("font-heading font-semibold text-lg leading-none", class),
            "data-slot": "card-title",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardDescription(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("text-muted-foreground text-sm", class),
            "data-slot": "card-description",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardAction(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "col-start-2 row-span-2 row-start-1 inline-flex self-start justify-self-end",
                class,
            ),
            "data-slot": "card-action",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardContent(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "flex-1 p-6 in-[[data-slot=card]:has(>[data-slot=card-header]:not(.border-b))]:pt-0 in-[[data-slot=card]:has(>[data-slot=card-footer]:not(.border-t))]:pb-0",
                class,
            ),
            "data-slot": "card-content",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardFooter(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "flex items-center p-6 in-[[data-slot=card]:has(>[data-slot=card-panel])]:pt-4",
                class,
            ),
            "data-slot": "card-footer",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardFrame(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "relative flex flex-col rounded-2xl border bg-card not-dark:bg-clip-padding text-card-foreground shadow-xs/5 [--clip-bottom:-1rem] [--clip-top:-1rem] before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-2xl)-1px)] before:bg-muted/72 before:shadow-[0_1px_--theme(--color-black/4%)] has-data-[slot=table-container]:overflow-hidden *:data-[slot=card]:-m-px *:data-[slot=table-container]:-m-px *:data-[slot=table-container]:w-[calc(100%+2px)] *:not-first:data-[slot=card]:rounded-t-xl *:not-last:data-[slot=card]:rounded-b-xl *:data-[slot=card]:bg-clip-padding *:data-[slot=card]:shadow-none *:data-[slot=card]:before:hidden *:not-first:data-[slot=card]:before:rounded-t-[calc(var(--radius-xl)-1px)] *:not-last:data-[slot=card]:before:rounded-b-[calc(var(--radius-xl)-1px)] dark:before:shadow-[0_-1px_--theme(--color-white/6%)] *:data-[slot=card]:[clip-path:inset(var(--clip-top)_1px_var(--clip-bottom)_1px_round_calc(var(--radius-2xl)-1px))] *:data-[slot=card]:last:[--clip-bottom:1px] *:data-[slot=card]:first:[--clip-top:1px]",
                class,
            ),
            "data-slot": "card-frame",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardFrameHeader(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "relative flex grid auto-rows-min grid-rows-[auto_auto] flex-col items-start gap-x-4 px-6 py-4 has-data-[slot=card-frame-action]:grid-cols-[1fr_auto]",
                class,
            ),
            "data-slot": "card-frame-header",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardFrameTitle(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("self-center font-semibold text-sm", class),
            "data-slot": "card-frame-title",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardFrameDescription(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("self-center text-muted-foreground text-sm", class),
            "data-slot": "card-frame-description",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardFrameAction(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "col-start-2 nth-3:row-span-2 nth-3:row-start-1 inline-flex self-center justify-self-end",
                class,
            ),
            "data-slot": "card-frame-action",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardFrameFooter(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!("px-6 py-4", class),
            "data-slot": "card-frame-footer",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn CardPanel(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "flex-1 p-6 in-[[data-slot=card]:has(>[data-slot=card-header]:not(.border-b))]:pt-0 in-[[data-slot=card]:has(>[data-slot=card-footer]:not(.border-t))]:pb-0",
                class,
            ),
            "data-slot": "card-panel",
            ..attributes,
            {children}
        }
    }
}
