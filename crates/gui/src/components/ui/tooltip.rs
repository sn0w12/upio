use dioxus::prelude::*;
use dioxus_primitives::tooltip::{self, TooltipContentProps, TooltipProps, TooltipTriggerProps};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use dioxus_primitives::{ContentAlign, ContentSide};

/// A contextual popup shown on hover or focus.
///
/// Ported from coss ui `tooltip`. The dioxus primitive renders the popup
/// without any positioning, so placement styles are applied here relative to
/// the [`Tooltip`] wrapper.
#[component]
pub fn Tooltip(props: TooltipProps) -> Element {
    let base = attributes!(div {
        class: "relative inline-flex max-w-fit",
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        tooltip::Tooltip {
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            disabled: props.disabled,
            attributes: merged,
            "data-slot": "tooltip",
            {props.children}
        }
    }
}

#[component]
pub fn TooltipTrigger(props: TooltipTriggerProps) -> Element {
    rsx! {
        tooltip::TooltipTrigger {
            id: props.id,
            r#as: props.r#as,
            attributes: props.attributes,
            "data-slot": "tooltip-trigger",
            {props.children}
        }
    }
}

/// Positioning classes for each side/align combination.
fn position_classes(side: ContentSide, align: ContentAlign) -> &'static str {
    match side {
        ContentSide::Top => match align {
            ContentAlign::Start => "bottom-full left-0 mb-1.5",
            ContentAlign::End => "bottom-full right-0 mb-1.5",
            ContentAlign::Center => "bottom-full left-1/2 mb-1.5 -translate-x-1/2",
        },
        ContentSide::Bottom => match align {
            ContentAlign::Start => "top-full left-0 mt-1.5",
            ContentAlign::End => "top-full right-0 mt-1.5",
            ContentAlign::Center => "top-full left-1/2 mt-1.5 -translate-x-1/2",
        },
        ContentSide::Left => match align {
            ContentAlign::Start => "right-full top-0 mr-1.5",
            ContentAlign::End => "right-full bottom-0 mr-1.5",
            ContentAlign::Center => "right-full top-1/2 mr-1.5 -translate-y-1/2",
        },
        ContentSide::Right => match align {
            ContentAlign::Start => "left-full top-0 ml-1.5",
            ContentAlign::End => "left-full bottom-0 ml-1.5",
            ContentAlign::Center => "left-full top-1/2 ml-1.5 -translate-y-1/2",
        },
    }
}

#[component]
pub fn TooltipContent(props: TooltipContentProps) -> Element {
    let base = attributes!(div {
        class: format!(
            "{} {}",
            position_classes(props.side, props.align),
            "absolute z-2500 w-max max-w-xs rounded-md border bg-popover not-dark:bg-clip-padding px-2 py-1 text-popover-foreground text-left text-balance text-xs shadow-md/5 data-[state=closed]:hidden data-[state=open]:animate-[toast-fade_.15s_ease-out]",
        ),
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        tooltip::TooltipContent {
            id: props.id,
            side: props.side,
            align: props.align,
            attributes: merged,
            "data-slot": "tooltip-popup",
            {props.children}
        }
    }
}
