use dioxus::prelude::*;
use dioxus_primitives::dialog::{self, DialogDescriptionProps, DialogTitleProps};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use tw_merge::*;

const CONTENT_CLASS: &str = "in-data-[state=open]:animate-[dialog-in_.15s_ease-out] in-data-[state=closed]:animate-[dialog-out_.15s_ease-in] relative flex max-h-full min-h-0 w-full min-w-0 max-w-lg origin-center flex-col rounded-2xl border bg-popover not-dark:bg-clip-padding text-popover-foreground opacity-[calc(1-var(--nested-dialogs))] shadow-lg/5 outline-none before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-2xl)-1px)] before:shadow-[0_1px_--theme(--color-black/4%)] sm:scale-[calc(1-0.1*var(--nested-dialogs))] dark:before:shadow-[0_-1px_--theme(--color-white/6%)]";

/// The props for the [`Dialog`] component.
#[derive(Props, Clone, PartialEq)]
pub struct DialogProps {
    /// The ID of the dialog root element.
    pub id: ReadSignal<Option<String>>,

    /// Whether the dialog is modal.
    #[props(default = ReadSignal::new(Signal::new(true)))]
    pub is_modal: ReadSignal<bool>,

    /// The controlled `open` state of the dialog.
    pub open: ReadSignal<Option<bool>>,

    /// The default `open` state if uncontrolled.
    #[props(default)]
    pub default_open: bool,

    /// A callback fired when the open state changes.
    #[props(default)]
    pub on_open_change: Callback<bool>,

    /// Extra classes merged onto the content panel, e.g. to widen the dialog.
    #[props(default)]
    pub class: String,

    /// Additional attributes for the dialog root element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// The children of the dialog.
    pub children: Element,
}

#[component]
pub fn Dialog(props: DialogProps) -> Element {
    rsx! {
        dialog::DialogRoot {
            class: "fixed inset-0 z-50 grid place-items-center overflow-y-auto bg-black/32 backdrop-blur-sm transition-opacity duration-150 data-[state=closed]:opacity-0",
            id: props.id,
            is_modal: props.is_modal,
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            attributes: props.attributes,
            dialog::DialogContent {
                class: tw_merge!(CONTENT_CLASS, props.class),
                "data-slot": "dialog-popup",
                {props.children}
            }
        }
    }
}

#[component]
pub fn DialogHeader(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "flex flex-col gap-2 p-6 in-[[data-slot=dialog-popup]:has([data-slot=dialog-panel])]:pb-3 max-sm:pb-4",
                class,
            ),
            "data-slot": "dialog-header",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn DialogFooter(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "flex flex-col-reverse gap-2 px-6 sm:flex-row sm:justify-end sm:rounded-b-[calc(var(--radius-2xl)-1px)] border-t bg-muted/72 py-4",
                class,
            ),
            "data-slot": "dialog-footer",
            ..attributes,
            {children}
        }
    }
}

#[component]
pub fn DialogTitle(props: DialogTitleProps) -> Element {
    let base = attributes!(h2 {
        class: "font-heading font-semibold text-xl leading-none",
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        dialog::DialogTitle { id: props.id, attributes: merged, {props.children} }
    }
}

#[component]
pub fn DialogDescription(props: DialogDescriptionProps) -> Element {
    let base = attributes!(p {
        class: "text-muted-foreground text-sm",
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        dialog::DialogDescription { id: props.id, attributes: merged, {props.children} }
    }
}

#[component]
pub fn DialogPanel(
    #[props(default)] class: String,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: tw_merge!(
                "p-6 in-[[data-slot=dialog-popup]:has([data-slot=dialog-header])]:pt-1 in-[[data-slot=dialog-popup]:has([data-slot=dialog-footer]:not(.border-t))]:pb-1",
                class,
            ),
            "data-slot": "dialog-panel",
            ..attributes,
            {children}
        }
    }
}
