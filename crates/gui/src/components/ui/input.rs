use dioxus::prelude::*;
use tw_merge::*;

#[derive(TwClass)]
#[tw(
    class = "h-8.5 w-full min-w-0 rounded-[inherit] px-[calc(--spacing(3)-1px)] text-foreground leading-8.5 outline-none placeholder:text-muted-foreground/72 sm:h-7.5 sm:leading-7.5 autofill:[-webkit-text-fill-color:var(--foreground)]"
)]
struct InputStyles {
    size: InputSize,
}

#[derive(PartialEq, TwVariant)]
pub enum InputSize {
    #[tw(default, class = "")]
    Default,
    #[tw(class = "h-7.5 px-[calc(--spacing(2.5)-1px)] leading-7.5 sm:h-6.5 sm:leading-6.5")]
    Sm,
    #[tw(class = "h-9.5 leading-9.5 sm:h-8.5 sm:leading-8.5")]
    Lg,
}

#[derive(TwClass)]
struct InputVariants {
    variant: InputVariant,
}

#[derive(PartialEq, TwVariant)]
pub enum InputVariant {
    #[tw(
        default,
        class = "relative inline-flex w-full rounded-lg border border-input bg-background not-dark:bg-clip-padding text-base shadow-xs/5 ring-ring/24 transition-shadow before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-lg)-1px)] not-has-disabled:not-has-focus-visible:not-has-aria-invalid:before:shadow-[0_1px_--theme(--color-black/4%)] has-focus-visible:has-aria-invalid:border-destructive/64 has-focus-visible:has-aria-invalid:ring-destructive/16 has-aria-invalid:border-destructive/36 has-focus-visible:border-ring has-autofill:bg-foreground/4 has-disabled:opacity-64 has-[:disabled,:focus-visible,[aria-invalid]]:shadow-none has-focus-visible:ring-[3px] sm:text-sm dark:bg-input/32 dark:has-autofill:bg-foreground/8 dark:has-aria-invalid:ring-destructive/24 dark:not-has-disabled:not-has-focus-visible:not-has-aria-invalid:before:shadow-[0_-1px_--theme(--color-white/6%)]"
    )]
    Default,
    #[tw(class = "")]
    Unstyled,
}

#[component]
pub fn Input(
    #[props(default)] size: InputSize,
    #[props(default)] variant: InputVariant,
    /// Extra classes merged onto the inner input element.
    #[props(default)]
    class: Option<String>,
    oninput: Option<EventHandler<FormEvent>>,
    onchange: Option<EventHandler<FormEvent>>,
    oninvalid: Option<EventHandler<FormEvent>>,
    onselect: Option<EventHandler<SelectionEvent>>,
    onselectionchange: Option<EventHandler<SelectionEvent>>,
    onfocus: Option<EventHandler<FocusEvent>>,
    onblur: Option<EventHandler<FocusEvent>>,
    onfocusin: Option<EventHandler<FocusEvent>>,
    onfocusout: Option<EventHandler<FocusEvent>>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    onkeypress: Option<EventHandler<KeyboardEvent>>,
    onkeyup: Option<EventHandler<KeyboardEvent>>,
    onwheel: Option<EventHandler<WheelEvent>>,
    oncompositionstart: Option<EventHandler<CompositionEvent>>,
    oncompositionupdate: Option<EventHandler<CompositionEvent>>,
    oncompositionend: Option<EventHandler<CompositionEvent>>,
    oncopy: Option<EventHandler<ClipboardEvent>>,
    oncut: Option<EventHandler<ClipboardEvent>>,
    onpaste: Option<EventHandler<ClipboardEvent>>,
    #[props(extends=GlobalAttributes)]
    #[props(extends=input)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        span {
            class: InputVariants { variant }.to_class(),
            "data-slot": "input-control",
            input {
                class: tw_merge!(InputStyles { size } .to_class(), class),
                oninput: move |e| _ = oninput.map(|callback| callback(e)),
                onchange: move |e| _ = onchange.map(|callback| callback(e)),
                oninvalid: move |e| _ = oninvalid.map(|callback| callback(e)),
                onselect: move |e| _ = onselect.map(|callback| callback(e)),
                onselectionchange: move |e| _ = onselectionchange.map(|callback| callback(e)),
                onfocus: move |e| _ = onfocus.map(|callback| callback(e)),
                onblur: move |e| _ = onblur.map(|callback| callback(e)),
                onfocusin: move |e| _ = onfocusin.map(|callback| callback(e)),
                onfocusout: move |e| _ = onfocusout.map(|callback| callback(e)),
                onkeydown: move |e| _ = onkeydown.map(|callback| callback(e)),
                onkeypress: move |e| _ = onkeypress.map(|callback| callback(e)),
                onkeyup: move |e| _ = onkeyup.map(|callback| callback(e)),
                onwheel: move |e| _ = onwheel.map(|callback| callback(e)),
                oncompositionstart: move |e| _ = oncompositionstart.map(|callback| callback(e)),
                oncompositionupdate: move |e| _ = oncompositionupdate.map(|callback| callback(e)),
                oncompositionend: move |e| _ = oncompositionend.map(|callback| callback(e)),
                oncopy: move |e| _ = oncopy.map(|callback| callback(e)),
                oncut: move |e| _ = oncut.map(|callback| callback(e)),
                onpaste: move |e| _ = onpaste.map(|callback| callback(e)),
                "data-slot": "input",
                ..attributes,
                {children}
            }
        }
    }
}
