use dioxus::prelude::*;
use dioxus_icons::lucide::{Check, ChevronDown};
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;
use dioxus_primitives::select::{self, SelectGroupLabelProps};
use tw_merge::*;

pub use dioxus_primitives::select::SelectGroup;

#[derive(TwClass)]
#[tw(
    class = "relative inline-flex min-h-9 w-full min-w-36 select-none items-center justify-between gap-2 rounded-lg border border-input bg-background not-dark:bg-clip-padding px-[calc(--spacing(3)-1px)] text-left text-base text-foreground shadow-xs/5 outline-none ring-ring/24 transition-shadow before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-lg)-1px)] not-focus-visible:not-aria-invalid:not-hover:before:shadow-[0_1px_--theme(--color-black/4%)] pointer-coarse:after:absolute pointer-coarse:after:size-full pointer-coarse:after:min-h-11 focus-visible:border-ring focus-visible:ring-[3px] aria-invalid:border-destructive/36 focus-visible:aria-invalid:border-destructive/64 focus-visible:aria-invalid:ring-destructive/16 sm:min-h-8 sm:text-sm dark:bg-input/32 dark:aria-invalid:ring-destructive/24 dark:not-focus-visible:not-aria-invalid:not-hover:before:shadow-[0_-1px_--theme(--color-white/6%)] [&_svg:not([class*='opacity-'])]:opacity-80 [&_svg:not([class*='size-'])]:size-4.5 sm:[&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0"
)]
pub struct SelectStyles {
    size: SelectSize,
}

#[derive(PartialEq, TwVariant)]
pub enum SelectSize {
    #[tw(default, class = "")]
    Default,
    #[tw(class = "min-h-10 sm:min-h-9")]
    Lg,
    #[tw(class = "min-h-8 gap-1.5 px-[calc(--spacing(2.5)-1px)] sm:min-h-7")]
    Sm,
}

/// Shared props for the trigger portion of a styled select.
fn select_trigger_class(size: SelectSize, class: Option<String>) -> String {
    tw_join!(SelectStyles { size }.to_class(), class)
}

/// Props for the styled [`Select`] wrapper.
#[derive(Props, Clone, PartialEq)]
pub struct SelectWrapperProps<T: Clone + PartialEq + 'static> {
    /// Size variant for the trigger.
    #[props(default)]
    pub size: SelectSize,
    /// The controlled value of the select.
    #[props(default)]
    pub value: Option<ReadSignal<Option<T>>>,
    /// The initial value of the select when uncontrolled.
    #[props(default)]
    pub default_value: Option<T>,
    /// Callback fired when the selected value changes.
    #[props(default)]
    pub on_value_change: Callback<Option<T>>,
    /// Shown while nothing is selected. Pin this to the current selection's
    /// label on controlled selects so trigger text never flickers while the
    /// popup option registry re-registers on open/close.
    #[props(default = ReadSignal::new(Signal::new("Select an option".to_string())))]
    pub placeholder: ReadSignal<String>,
    /// Whether interaction with the select is disabled.
    #[props(default = ReadSignal::new(Signal::new(false)))]
    pub disabled: ReadSignal<bool>,
    /// Additional attributes for the root element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
    /// The children of the select.
    pub children: Element,
}

/// A styled single-select wrapper around [`select::Select`].
#[component]
pub fn Select<T: Clone + PartialEq + 'static>(props: SelectWrapperProps<T>) -> Element {
    let base = attributes!(div { class: "relative" });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        select::Select {
            disabled: props.disabled,
            default_value: props.default_value,
            on_value_change: props.on_value_change,
            attributes: merged,
            select::SelectTrigger { class: select_trigger_class(props.size, None),
                select::SelectValue {
                    placeholder: props.placeholder,
                    class: "flex-1 truncate data-[placeholder=true]:text-muted-foreground",
                }
                ChevronDown { class: "-me-1 size-4.5 opacity-80 sm:size-4" }
            }
            select::SelectList { class: "absolute z-1000 left-0 top-full min-w-full box-border p-1 rounded-lg border bg-popover not-dark:bg-clip-padding shadow-lg/5 before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-lg)-1px)] before:shadow-[0_1px_--theme(--color-black/4%)] dark:before:shadow-[0_-1px_--theme(--color-white/6%)]",
                {props.children}
            }
        }
    }
}

/// Props for the styled [`SelectMulti`] wrapper.
#[derive(Props, Clone, PartialEq)]
pub struct SelectMultiWrapperProps<T: Clone + PartialEq + 'static> {
    /// Size variant for the trigger.
    #[props(default)]
    pub size: SelectSize,
    /// The controlled list of selected values.
    #[props(default)]
    pub values: ReadSignal<Option<Vec<T>>>,
    /// The default list of selected values.
    #[props(default)]
    pub default_values: Vec<T>,
    /// Callback when the list of selected values changes.
    #[props(default)]
    pub on_values_change: Callback<Vec<T>>,
    /// Shown while nothing is selected. Pin this to the current selection's
    /// labels on controlled multi-selects so trigger text never flickers.
    #[props(default = ReadSignal::new(Signal::new("Select options".to_string())))]
    pub placeholder: ReadSignal<String>,
    /// Whether interaction with the multi-select is disabled.
    #[props(default = ReadSignal::new(Signal::new(false)))]
    pub disabled: ReadSignal<bool>,
    /// Additional attributes for the root element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
    /// The children of the select.
    pub children: Element,
}

/// A styled multi-select wrapper around [`select::SelectMulti`].
#[component]
pub fn SelectMulti<T: Clone + PartialEq + 'static>(props: SelectMultiWrapperProps<T>) -> Element {
    let base = attributes!(div { class: "relative" });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        select::SelectMulti {
            values: props.values,
            default_values: props.default_values,
            disabled: props.disabled,
            attributes: merged,
            select::SelectTrigger { class: select_trigger_class(props.size, None),
                select::SelectValue {
                    placeholder: props.placeholder,
                    class: "flex-1 truncate data-[placeholder=true]:text-muted-foreground",
                }
                ChevronDown { class: "-me-1 size-4.5 opacity-80 sm:size-4" }
            }
            select::SelectList { class: "absolute z-1000 left-0 top-full min-w-full box-border p-1 rounded-lg border bg-popover not-dark:bg-clip-padding shadow-lg/5 before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-lg)-1px)] before:shadow-[0_1px_--theme(--color-black/4%)] dark:before:shadow-[0_-1px_--theme(--color-white/6%)]",
                {props.children}
            }
        }
    }
}

#[component]
pub fn SelectGroupLabel(props: SelectGroupLabelProps) -> Element {
    let base = attributes!(div {
        class: "px-2 py-1.5 font-medium text-muted-foreground text-xs"
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        select::SelectGroupLabel { id: props.id, attributes: merged, {props.children} }
    }
}

/// Props for the styled [`SelectOption`] wrapper.
#[derive(Props, Clone, PartialEq)]
pub struct SelectOptionProps<T: Clone + PartialEq + 'static> {
    /// The value of the option.
    pub value: ReadSignal<T>,
    /// The index of the option in the list.
    pub index: ReadSignal<usize>,
    /// The text value of the option used for typeahead search.
    #[props(default)]
    pub text_value: Option<String>,
    /// Optional ID for the option.
    #[props(default)]
    pub id: Option<String>,
    /// Optional label for the option (for accessibility).
    #[props(default)]
    pub aria_label: Option<String>,
    /// Optional description role for the option (for accessibility).
    #[props(default)]
    pub aria_roledescription: Option<String>,
    /// Whether the option can be selected.
    #[props(default)]
    pub disabled: bool,
    /// Additional attributes for the option element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
    /// The children to render inside the option.
    pub children: Element,
}

#[component]
pub fn SelectOption<T: Clone + PartialEq + 'static>(props: SelectOptionProps<T>) -> Element {
    // The primitive emits `data-disabled` as "true"/"false" on every option
    // (never a bare presence attribute), so disabled styling must compare the
    // value. Keyboard focus lands on the focused option via tabindex, so
    // `focus:` covers what Base UI does with `data-highlighted`.
    let base = attributes!(div { class: "grid min-h-8 in-data-[side=none]:min-w-[calc(var(--anchor-width)+1.25rem)] cursor-default grid-cols-[1rem_1fr] items-center gap-2 rounded-sm py-1 ps-2 pe-4 text-base outline-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground focus:outline-none data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-64 sm:min-h-7 sm:text-sm [&_svg:not([class*='size-'])]:size-4.5 sm:[&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0" });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        select::SelectOption::<T> {
            value: props.value,
            text_value: props.text_value,
            disabled: props.disabled,
            id: props.id,
            index: props.index,
            aria_label: props.aria_label,
            aria_roledescription: props.aria_roledescription,
            attributes: merged,
            select::SelectItemIndicator {
                Check { class: "col-start-1" }
            }
            div { class: "col-start-2 min-w-0", {props.children} }
        }
    }
}
