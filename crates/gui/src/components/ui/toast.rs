use dioxus::prelude::*;
use dioxus_icons::lucide::{CircleAlert, CircleCheck, Info, TriangleAlert, X};
use std::time::Duration;
use tw_merge::*;

use crate::components::ui::button::{Button, ButtonSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastType {
    Success,
    Error,
    Warning,
    Info,
}

impl ToastType {
    fn as_str(self) -> &'static str {
        match self {
            ToastType::Success => "success",
            ToastType::Error => "error",
            ToastType::Warning => "warning",
            ToastType::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToastOptions {
    pub description: Option<String>,
    pub duration: Option<Duration>,
    pub permanent: bool,
}

impl ToastOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn description(mut self, description: impl ToString) -> Self {
        self.description = Some(description.to_string());
        self
    }
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
    pub fn permanent(mut self, permanent: bool) -> Self {
        self.permanent = permanent;
        self
    }
}

#[derive(Debug, Clone)]
struct ToastItem {
    id: usize,
    toast_type: ToastType,
    title: String,
    description: Option<String>,
    permanent: bool,
    exiting: bool,
}

#[derive(Debug, Clone, Copy)]
struct ToastManager {
    toasts: Signal<Vec<ToastItem>>,
    default_duration: Duration,
    max_toasts: usize,
    next_id: Signal<usize>,
}

impl ToastManager {
    fn show(&mut self, title: String, toast_type: ToastType, options: ToastOptions) {
        let id = {
            let mut next = self.next_id.write();
            let id = *next;
            *next += 1;
            id
        };
        let item = ToastItem {
            id,
            toast_type,
            title,
            description: options.description,
            permanent: options.permanent,
            exiting: false,
        };
        {
            let mut toasts = self.toasts.write();
            // Keep the new toast, dismissing the oldest non-permanent toast if at the limit.
            while toasts.len() >= self.max_toasts {
                if let Some(pos) = toasts.iter().rposition(|t| !t.permanent && !t.exiting) {
                    toasts.remove(pos);
                } else {
                    break;
                }
            }
            toasts.insert(0, item);
        }
        if !options.permanent {
            let duration = options.duration.unwrap_or(self.default_duration);
            let mut toasts = self.toasts;
            spawn(async move {
                tokio::time::sleep(duration).await;
                let mut toasts = toasts.write();
                if let Some(t) = toasts.iter_mut().find(|t| t.id == id) {
                    if !t.exiting {
                        t.exiting = true;
                    }
                }
            });
        }
    }

    fn dismiss(&mut self, id: usize) {
        let mut toasts = self.toasts.write();
        if let Some(t) = toasts.iter_mut().find(|t| t.id == id) {
            t.exiting = true;
        }
    }
}

/// A handle to push toasts. Get it with [`use_toast`].
#[derive(Clone, Copy)]
pub struct Toasts(ToastManager);

impl Toasts {
    pub fn show(&self, title: String, toast_type: ToastType, options: ToastOptions) {
        self.0.clone().show(title, toast_type, options);
    }
    /// Dismiss a toast by id, animating it out.
    pub fn dismiss(&self, id: usize) {
        self.0.clone().dismiss(id);
    }
    pub fn success(&self, title: String, options: ToastOptions) {
        self.show(title, ToastType::Success, options);
    }
    pub fn error(&self, title: String, options: ToastOptions) {
        self.show(title, ToastType::Error, options);
    }
    pub fn warning(&self, title: String, options: ToastOptions) {
        self.show(title, ToastType::Warning, options);
    }
    pub fn info(&self, title: String, options: ToastOptions) {
        self.show(title, ToastType::Info, options);
    }
}

/// Get a handle to push toasts. Must be called inside a [`ToastProvider`].
pub fn use_toast() -> Toasts {
    let manager = use_context::<ToastManager>();
    Toasts(manager)
}

/// Position of the toast viewport within the window.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastPosition {
    #[default]
    BottomRight,
    BottomLeft,
    BottomCenter,
    TopRight,
    TopLeft,
    TopCenter,
}

impl ToastPosition {
    fn viewport_class(self) -> &'static str {
        match self {
            ToastPosition::TopLeft => "left-8 top-8",
            ToastPosition::TopCenter => "left-1/2 top-8 -translate-x-1/2",
            ToastPosition::TopRight => "right-8 top-8",
            ToastPosition::BottomLeft => "bottom-8 left-8",
            ToastPosition::BottomCenter => "bottom-8 left-1/2 -translate-x-1/2",
            ToastPosition::BottomRight => "bottom-8 right-8",
        }
    }
    fn enter_class(self) -> &'static str {
        "animate-[toast-fade_.25s_ease-out]"
    }
}

/// Provides a toast notification system to the subtree.
///
/// Wrap your app (or a view) in this provider. Any child can call [`use_toast`]
/// to push a notification.
#[component]
pub fn ToastProvider(
    #[props(default = ReadSignal::new(Signal::new(Duration::from_secs(5))))]
    default_duration: ReadSignal<Duration>,
    #[props(default = ReadSignal::new(Signal::new(5)))] max_toasts: ReadSignal<usize>,
    #[props(default)] position: ToastPosition,
    children: Element,
) -> Element {
    let mut toasts = use_signal(Vec::<ToastItem>::new);
    let next_id = use_signal(|| 0usize);
    let heights = use_signal(std::collections::HashMap::<usize, f64>::new);
    let default_duration = default_duration();
    let max_toasts = max_toasts();

    use_context_provider(move || ToastManager {
        toasts,
        default_duration,
        max_toasts,
        next_id,
    });

    // Remove toasts that have finished their exit animation.
    use_effect(move || {
        if toasts().iter().any(|t| t.exiting) {
            spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                toasts.write().retain(|t| !t.exiting);
            });
        }
    });

    let gap = 12.0;
    let is_top = matches!(
        position,
        ToastPosition::TopLeft | ToastPosition::TopCenter | ToastPosition::TopRight
    );

    let rendered = toasts();
    let offsets = use_memo(move || {
        let toasts = toasts();
        let heights = heights();
        let mut offsets = Vec::with_capacity(toasts.len());
        let mut acc = 0.0;
        for toast in &toasts {
            offsets.push(acc);
            acc += heights.get(&toast.id).copied().unwrap_or(80.0) + gap;
        }
        offsets
    });

    let dismiss = use_callback(move |id: usize| {
        let mut toasts = toasts.write();
        if let Some(t) = toasts.iter_mut().find(|t| t.id == id) {
            t.exiting = true;
        }
    });

    rsx! {
        {children}
        div {
            class: tw_join!(
                "pointer-events-none fixed z-60 w-80 max-w-[calc(100vw-var(--spacing(8))*2)]",
                position.viewport_class(),
            ),
            for (index, toast) in rendered.iter().enumerate() {
                ToastView {
                    key: "{toast.id}",
                    element_id: "toast-{toast.id}",
                    toast_type: toast.toast_type,
                    title: toast.title.clone(),
                    description: toast.description.clone(),
                    enter_class: position.enter_class(),
                    is_top,
                    offset: offsets()[index],
                    exiting: toast.exiting,
                    on_close: {
                        let toast_id = toast.id;
                        move |_| dismiss.call(toast_id)
                    },
                    on_height: {
                        let toast_id = toast.id;
                        let mut heights = heights;
                        move |h: f64| {
                            heights.write().insert(toast_id, h);
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn ToastView(
    element_id: String,
    toast_type: ToastType,
    title: String,
    description: Option<String>,
    enter_class: &'static str,
    is_top: bool,
    offset: f64,
    exiting: bool,
    on_close: EventHandler<MouseEvent>,
    on_height: EventHandler<f64>,
) -> Element {
    let icon = match toast_type {
        ToastType::Success => rsx! {
            CircleCheck { class: "size-4 text-success" }
        },
        ToastType::Error => rsx! {
            CircleAlert { class: "size-4 text-destructive" }
        },
        ToastType::Warning => rsx! {
            TriangleAlert { class: "size-4 text-warning" }
        },
        ToastType::Info => rsx! {
            Info { class: "size-4 text-info" }
        },
    };

    let bottom_anchor = !is_top;
    let translate_y = if bottom_anchor { -offset } else { offset };
    let position_class = if bottom_anchor { "bottom-0" } else { "top-0" };
    let exit_transform = if bottom_anchor {
        "transform-[translateY(calc(var(--toast-offset)+100%))]"
    } else {
        "transform-[translateY(calc(var(--toast-offset)-100%))]"
    };

    rsx! {
        div {
            id: "{element_id}",
            "data-type": toast_type.as_str(),
            style: "--toast-offset: {translate_y}px",
            class: tw_join!(
                "pointer-events-auto absolute left-0 right-0 flex items-center justify-between gap-1.5 overflow-hidden rounded-lg border bg-popover not-dark:bg-clip-padding text-popover-foreground text-sm shadow-lg/5 before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-lg)-1px)] before:shadow-[0_1px_--theme(--color-black/4%)] dark:before:shadow-[0_-1px_--theme(--color-white/6%)]",
                position_class, if exiting {
                format!("opacity-0 transition-[transform,opacity] duration-300 ease-in {exit_transform}")
                } else {
                "transition-transform duration-500 ease-[cubic-bezier(.22,1,.36,1)] transform-[translateY(var(--toast-offset))]"
                .to_string() }, if exiting { "" } else { enter_class },
            ),
            onmounted: move |_| {
                let mut eval = dioxus::document::eval(
                    &format!(
                        r#"
                            const el = document.getElementById("{element_id}");
                            if (!el) {{ dioxus.send(80); return; }}
                            if (el.__upioRoCleanup) {{ el.__upioRoCleanup(); }}
                            const ro = new ResizeObserver(() => dioxus.send(el.offsetHeight));
                            ro.observe(el);
                            el.__upioRoCleanup = () => ro.disconnect();
                            dioxus.send(el.offsetHeight);
                            "#,
                    ),
                );
                let on_height = on_height;
                spawn(async move {
                    while let Ok(h) = eval.recv::<f64>().await {
                        on_height.call(h);
                    }
                });
                let cleanup_id = element_id.clone();
                dioxus::core::use_drop(move || {
                    _ = dioxus::document::eval(
                        &format!(
                            "const el = document.getElementById(\"{cleanup_id}\"); \
                                     if (el && el.__upioRoCleanup) {{ el.__upioRoCleanup(); }}",
                        ),
                    );
                });
            },
            div { class: "flex gap-2 px-3.5 py-3",
                div { class: "[&_svg]:shrink-0 [&_svg]:pointer-events-none", {icon} }
                div { class: "flex flex-col gap-0.5",
                    div { class: "font-medium", "{title}" }
                    if let Some(description) = description {
                        div { class: "text-muted-foreground", "{description}" }
                    }
                }
            }
            Button {
                onclick: move |e| on_close.call(e),
                size: ButtonSize::IconSm,
                class: "mr-2",
                aria_label: "Close notification",
                r#type: "button",
                X {}
            }
        }
    }
}
