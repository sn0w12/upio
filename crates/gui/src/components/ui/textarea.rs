use dioxus::prelude::*;
use tw_merge::*;

#[derive(TwClass)]
#[tw(
    class = "field-sizing-content min-h-17.5 w-full rounded-[inherit] px-[calc(--spacing(3)-1px)] py-[calc(--spacing(1.5)-1px)] text-foreground outline-none placeholder:text-muted-foreground/72 max-sm:min-h-20.5"
)]
struct TextareaStyles {
    size: TextareaSize,
}

#[derive(PartialEq, TwVariant)]
pub enum TextareaSize {
    #[tw(default, class = "")]
    Default,
    #[tw(
        class = "min-h-16.5 px-[calc(--spacing(2.5)-1px)] py-[calc(--spacing(1)-1px)] max-sm:min-h-19.5"
    )]
    Sm,
    #[tw(class = "min-h-18.5 py-[calc(--spacing(2)-1px)] max-sm:min-h-21.5")]
    Lg,
}

#[derive(TwClass)]
struct TextareaVariants {
    variant: TextareaVariant,
}

#[derive(PartialEq, TwVariant)]
pub enum TextareaVariant {
    #[tw(
        default,
        class = "relative inline-flex w-full rounded-lg border border-input bg-background not-dark:bg-clip-padding text-base shadow-xs/5 ring-ring/24 transition-shadow before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-lg)-1px)] has-focus-visible:has-aria-invalid:border-destructive/64 has-focus-visible:has-aria-invalid:ring-destructive/16 has-aria-invalid:border-destructive/36 has-focus-visible:border-ring has-disabled:opacity-64 has-[:disabled,:focus-visible,[aria-invalid]]:shadow-none has-focus-visible:ring-[3px] not-has-disabled:has-not-focus-visible:not-has-aria-invalid:before:shadow-[0_1px_--theme(--color-black/4%)] sm:text-sm dark:bg-input/32 dark:has-aria-invalid:ring-destructive/24 dark:not-has-disabled:has-not-focus-visible:not-has-aria-invalid:before:shadow-[0_-1px_--theme(--color-white/6%)]"
    )]
    Default,
    #[tw(class = "")]
    Unstyled,
}

#[component]
pub fn Textarea(
    #[props(default)] class: Option<String>,
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
    oncompositionstart: Option<EventHandler<CompositionEvent>>,
    oncompositionupdate: Option<EventHandler<CompositionEvent>>,
    oncompositionend: Option<EventHandler<CompositionEvent>>,
    oncopy: Option<EventHandler<ClipboardEvent>>,
    oncut: Option<EventHandler<ClipboardEvent>>,
    onpaste: Option<EventHandler<ClipboardEvent>>,
    onmounted: Option<EventHandler<MountedEvent>>,
    #[props(default)] size: TextareaSize,
    #[props(default)] variant: TextareaVariant,
    #[props(extends=GlobalAttributes)]
    #[props(extends=textarea)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        span {
            class: TextareaVariants { variant }.to_class(),
            "data-slot": "textarea-control",
            textarea {
                class: tw_merge!(TextareaStyles { size } .to_class(), class),
                "data-slot": "textarea",
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
                oncompositionstart: move |e| _ = oncompositionstart.map(|callback| callback(e)),
                oncompositionupdate: move |e| _ = oncompositionupdate.map(|callback| callback(e)),
                oncompositionend: move |e| _ = oncompositionend.map(|callback| callback(e)),
                oncopy: move |e| _ = oncopy.map(|callback| callback(e)),
                oncut: move |e| _ = oncut.map(|callback| callback(e)),
                onpaste: move |e| _ = onpaste.map(|callback| callback(e)),
                onmounted: move |e| _ = onmounted.map(|callback| callback(e)),
                ..attributes,
                {children}
            }
        }
    }
}
