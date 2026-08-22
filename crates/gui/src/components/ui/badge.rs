use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;
use tw_merge::*;

#[derive(TwClass)]
#[tw(
    class = "relative inline-flex shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded-sm border border-transparent font-medium outline-none transition-shadow focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-64 [&_svg:not([class*='opacity-'])]:opacity-80 [&_svg:not([class*='size-'])]:size-3.5 sm:[&_svg:not([class*='size-'])]:size-3 [&_svg]:pointer-events-none [&_svg]:shrink-0 [button&,a&]:cursor-pointer [button&,a&]:pointer-coarse:after:absolute [button&,a&]:pointer-coarse:after:size-full [button&,a&]:pointer-coarse:after:min-h-11 [button&,a&]:pointer-coarse:after:min-w-11"
)]
pub struct BadgeStyles {
    size: BadgeSize,
    variant: BadgeVariant,
}

#[derive(PartialEq, TwVariant)]
pub enum BadgeSize {
    #[tw(
        default,
        class = "h-5.5 min-w-5.5 px-[calc(--spacing(1)-1px)] text-sm sm:h-4.5 sm:min-w-4.5 sm:text-xs"
    )]
    Default,
    #[tw(
        class = "h-6.5 min-w-6.5 px-[calc(--spacing(1.5)-1px)] text-base sm:h-5.5 sm:min-w-5.5 sm:text-sm"
    )]
    Lg,
    #[tw(
        class = "h-5 min-w-5 rounded-[.25rem] px-[calc(--spacing(1)-1px)] text-xs sm:h-4 sm:min-w-4 sm:text-[.625rem]"
    )]
    Sm,
}

#[derive(PartialEq, TwVariant)]
pub enum BadgeVariant {
    #[tw(
        default,
        class = "bg-primary text-primary-foreground [button&,a&]:hover:bg-primary/90"
    )]
    Default,
    #[tw(class = "bg-destructive text-white [button&,a&]:hover:bg-destructive/90")]
    Destructive,
    #[tw(class = "bg-destructive/8 text-destructive-foreground dark:bg-destructive/16")]
    Error,
    #[tw(class = "bg-info/8 text-info-foreground dark:bg-info/16")]
    Info,
    #[tw(
        class = "border-input bg-background text-foreground dark:bg-input/32 [button&,a&]:hover:bg-accent/50 dark:[button&,a&]:hover:bg-input/48"
    )]
    Outline,
    #[tw(class = "bg-secondary text-secondary-foreground [button&,a&]:hover:bg-secondary/90")]
    Secondary,
    #[tw(class = "bg-success/8 text-success-foreground dark:bg-success/16")]
    Success,
    #[tw(class = "bg-warning/8 text-warning-foreground dark:bg-warning/16")]
    Warning,
}

#[component]
pub fn Badge(
    #[props(default)] variant: BadgeVariant,
    #[props(default)] size: BadgeSize,
    #[props(extends=GlobalAttributes)]
    #[props(extends=span)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(button {
        class: BadgeStyles { variant, size }.to_class(),
    });
    let merged = merge_attributes(vec![base, attributes]);

    rsx! {
        span { ..merged,{children} }
    }
}
