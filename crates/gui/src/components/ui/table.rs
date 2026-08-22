use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;
use tw_merge::*;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum TableVariant {
    #[default]
    Default,
    Card,
}

#[component]
pub fn Table(
    #[props(default)] variant: TableVariant,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    class: Option<String>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: tw_join!("relative w-full overflow-x-auto", class),
        "data-slot": "table-container",
        "data-variant": if variant == TableVariant::Card { "card" } else { "default" },
    });
    let merged = merge_attributes(vec![base, attributes]);

    rsx! {
        div {..merged,
            table {
                "data-slot": "table",
                class: "w-full caption-bottom in-data-[variant=card]:border-separate in-data-[variant=card]:border-spacing-0 text-sm",
                {children}
            }
        }
    }
}

#[component]
pub fn TableHeader(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        thead {
            class: tw_merge!("[&_tr]:border-b", class),
            "data-slot": "table-header",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn TableBody(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        tbody {
            class: tw_merge!(
                "relative in-data-[variant=card]:rounded-xl in-data-[variant=card]:shadow-xs/5 before:pointer-events-none before:absolute before:inset-px not-in-data-[variant=card]:before:hidden before:rounded-[calc(var(--radius-xl)-1px)] before:shadow-[0_1px_--theme(--color-black/4%)] dark:before:shadow-[0_-1px_--theme(--color-white/8%)] [&_tr:last-child]:border-0 in-data-[variant=card]:*:[tr]:border-0 in-data-[variant=card]:*:[tr]:*:[td]:border-b in-data-[variant=card]:*:[tr]:*:[td]:bg-card in-data-[variant=card]:*:[tr]:first:*:[td]:first:rounded-ss-xl in-data-[variant=card]:*:[tr]:*:[td]:first:border-s in-data-[variant=card]:*:[tr]:first:*:[td]:border-t in-data-[variant=card]:*:[tr]:last:*:[td]:last:rounded-ee-xl in-data-[variant=card]:*:[tr]:*:[td]:last:border-e in-data-[variant=card]:*:[tr]:first:*:[td]:last:rounded-se-xl in-data-[variant=card]:*:[tr]:last:*:[td]:first:rounded-es-xl in-data-[variant=card]:*:[tr]:hover:*:[td]:bg-[color-mix(in_srgb,var(--card),var(--color-black)_2%)] in-data-[variant=card]:*:[tr]:data-[state=selected]:*:[td]:bg-[color-mix(in_srgb,var(--card),var(--color-black)_4%)] dark:in-data-[variant=card]:*:[tr]:data-[state=selected]:*:[td]:bg-[color-mix(in_srgb,var(--card),var(--color-white)_4%)] dark:in-data-[variant=card]:*:[tr]:hover:*:[td]:bg-[color-mix(in_srgb,var(--card),var(--color-white)_2%)]",
                class
            ),
            "data-slot": "table-body",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn TableFooter(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        tfoot {
            class: tw_merge!(
                "border-t in-data-[variant=card]:border-none bg-transparent not-in-data-[variant=card]:bg-[color-mix(in_srgb,var(--card),var(--color-black)_2%)] font-medium dark:not-in-data-[variant=card]:bg-[color-mix(in_srgb,var(--card),var(--color-white)_2%)] [&>tr]:last:border-b-0",
                class
            ),
            "data-slot": "table-footer",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn TableRow(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        tr {
            class: tw_merge!(
                "relative border-b not-in-data-[variant=card]:hover:bg-[color-mix(in_srgb,var(--background),var(--color-black)_2%)] not-in-data-[variant=card]:data-[state=selected]:bg-[color-mix(in_srgb,var(--background),var(--color-black)_4%)] dark:not-in-data-[variant=card]:data-[state=selected]:bg-[color-mix(in_srgb,var(--background),var(--color-white)_4%)] dark:not-in-data-[variant=card]:hover:bg-[color-mix(in_srgb,var(--background),var(--color-white)_2%)]",
                class
            ),
            "data-slot": "table-row",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn TableHead(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        th {
            class: tw_merge!(
                "h-10 whitespace-nowrap px-2.5 text-left align-middle font-medium text-muted-foreground leading-none has-[[role=checkbox]]:w-px last:has-[[role=checkbox]]:ps-0 first:has-[[role=checkbox]]:pe-0",
                class,
            ),
            "data-slot": "table-head",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn TableCell(
    #[props(default)] class: Option<String>,
    /// Number of columns this cell spans, when not 1.
    #[props(default)]
    colspan: Option<i64>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        td {
            class: tw_merge!(
                "whitespace-nowrap bg-clip-padding p-2.5 in-data-[slot=table-footer]:py-3.5 align-middle leading-none in-data-[variant=card]:first:ps-[calc(--spacing(2.5)-1px)] in-data-[variant=card]:last:pe-[calc(--spacing(2.5)-1px)] has-[[role=checkbox]]:w-px last:has-[[role=checkbox]]:ps-0 first:has-[[role=checkbox]]:pe-0",
                class,
            ),
            "data-slot": "table-cell",
            colspan: colspan.unwrap_or(1),
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn TableCaption(
    #[props(default)] class: Option<String>,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        caption {
            class: tw_merge!("in-data-[variant=card]:my-4 mt-4 text-muted-foreground text-sm", class),
            "data-slot": "table-caption",
            ..attributes,
            {children}
        }
    }
}
