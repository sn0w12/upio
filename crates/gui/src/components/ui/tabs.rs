use dioxus::prelude::*;
use dioxus_primitives::tabs::{self, TabContentProps};
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use tw_merge::*;

/// The props for the [`Tabs`] component.
#[derive(Props, Clone, PartialEq)]
pub struct TabsProps {
    /// The class of the tabs component.
    #[props(default)]
    pub class: String,

    /// The controlled value of the active tab.
    pub value: ReadSignal<Option<String>>,

    /// The default active tab value when uncontrolled.
    #[props(default)]
    pub default_value: String,

    /// Callback fired when the active tab changes.
    #[props(default)]
    pub on_value_change: Callback<String>,

    /// Whether the tabs are disabled.
    #[props(default)]
    pub disabled: ReadSignal<bool>,

    /// Whether the tabs are horizontal. Defaults to `true`.
    #[props(default = ReadSignal::new(Signal::new(true)))]
    pub horizontal: ReadSignal<bool>,

    /// Whether focus should loop around when reaching the end.
    #[props(default = ReadSignal::new(Signal::new(true)))]
    pub roving_loop: ReadSignal<bool>,

    /// Additional attributes to apply to the tabs element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// The children of the tabs component.
    pub children: Element,
}

#[derive(TwClass)]
#[tw(class = "relative z-0 flex w-fit items-center justify-center gap-x-0.5 text-muted-foreground")]
struct TabsListStyles {
    variant: TabsVariant,
}

#[derive(PartialEq, TwVariant)]
pub enum TabsVariant {
    #[tw(default, class = "rounded-lg bg-muted p-0.5 text-muted-foreground/72")]
    Default,
    #[tw(
        class = "gap-x-1 *:data-[slot=tabs-tab]:rounded-b-none *:data-[slot=tabs-tab]:hover:bg-accent/64"
    )]
    Underlined,
}

#[derive(TwClass)]
#[tw(class = "")]
struct TabsSizes {
    size: TabsSize,
}

#[derive(PartialEq, TwVariant)]
pub enum TabsSize {
    #[tw(default, class = "h-8.5 px-[calc(--spacing(2.5)-1px)] sm:h-7.5")]
    Default,
    #[tw(class = "h-9.5 px-[calc(--spacing(3)-1px)] sm:h-8.5")]
    Lg,
    #[tw(class = "h-7.5 px-[calc(--spacing(2)-1px)] sm:h-6.5")]
    Sm,
}

#[component]
pub fn Tabs(props: TabsProps) -> Element {
    let base = attributes!(div {
        class: format!(
            "{} {}",
            props.class, "flex flex-col gap-2 data-[orientation=vertical]:flex-row",
        ),
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        tabs::Tabs {
            value: props.value,
            default_value: props.default_value,
            on_value_change: props.on_value_change,
            disabled: props.disabled,
            horizontal: props.horizontal,
            roving_loop: props.roving_loop,
            attributes: merged,
            "data-slot": "tabs",
            {props.children}
        }
    }
}

#[component]
pub fn TabList(
    #[props(default)] variant: TabsVariant,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: match variant {
            // A bottom hairline that the active tab's underline merges into.
            TabsVariant::Underlined =>
                format!("{} {}", TabsListStyles { variant }.to_class(), "border-b",),
            TabsVariant::Default => TabsListStyles { variant }.to_class(),
        },
    });
    let merged = merge_attributes(vec![base, attributes]);

    rsx! {
        tabs::TabList { attributes: merged, "data-slot": "tabs-list", {children} }
    }
}

#[component]
pub fn TabTrigger(
    #[props(default)] disabled: ReadSignal<bool>,
    #[props(default)] size: TabsSize,
    #[props(default)] variant: TabsVariant,
    #[props(extends=GlobalAttributes)] attributes: Vec<Attribute>,
    index: ReadSignal<usize>,
    value: String,
    children: Element,
) -> Element {
    // The dioxus primitive exposes state via data-state="active|inactive" and
    // data-disabled="true|false"; there is no animated indicator element, so
    // the active affordance is styled directly on the trigger.
    let variant_classes = match variant {
        TabsVariant::Default => tw_join!(
            "data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm/5 dark:data-[state=active]:bg-input dark:data-[state=active]:shadow-none",
        ),
        TabsVariant::Underlined => tw_join!(
            "rounded-none pb-1.5 pt-1 data-[state=active]:text-foreground data-[state=active]:shadow-[inset_0_-2px_0_0_var(--color-primary)]",
        ),
    };

    rsx! {
        tabs::TabTrigger {
            class: tw_merge!(
                "relative flex shrink-0 cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap rounded-md border border-transparent font-medium text-base outline-none transition-[color,background-color,box-shadow] hover:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-64 sm:text-sm",
                "[&_svg:not([class*='opacity-'])]:opacity-80 [&_svg:not([class*='size-'])]:size-4.5 sm:[&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:-mx-0.5 [&_svg]:shrink-0",
                TabsSizes { size } .to_class(), variant_classes,
            ),
            value,
            index,
            disabled,
            attributes,
            "data-slot": "tabs-tab",
            {children}
        }
    }
}

#[component]
pub fn TabContent(props: TabContentProps) -> Element {
    let base = attributes!(div {
        class: format!(
            "{} {}",
            props.class.unwrap_or_default(),
            "flex-1 outline-none",
        )
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        tabs::TabContent {
            class: None,
            value: props.value,
            id: props.id,
            index: props.index,
            attributes: merged,
            "data-slot": "tabs-content",
            {props.children}
        }
    }
}
