use std::time::Duration;

use dioxus::prelude::*;
use dioxus_icons::lucide::{Check, Copy};
use tw_merge::*;

use crate::components::ui::button::{button_class, ButtonSize, ButtonVariant};
use crate::utils::copy_text;

/// A copy-to-clipboard icon button that flashes a checkmark after copying.
///
/// `children` replace the icons when you need a labeled variant; by default
/// the button shows `Copy`, then `Check` for 1.5 seconds.
#[component]
pub fn CopyButton(
    /// The text placed on the clipboard.
    text: String,
    /// Extra classes merged onto the button.
    #[props(default)]
    class: Option<String>,
    /// Visual style; defaults to a small ghost icon button.
    #[props(default)]
    variant: CopyButtonVariant,
    /// Content rendered instead of the icons (e.g. a "Copy all URLs" label).
    #[props(default)]
    children: Option<Element>,
) -> Element {
    let mut generation = use_signal(|| 0u64);
    let mut copied = use_signal(|| false);

    let base = match variant {
        CopyButtonVariant::Ghost => button_class(ButtonSize::IconSm, ButtonVariant::Ghost),
        CopyButtonVariant::Outline => button_class(ButtonSize::Sm, ButtonVariant::Outline),
    };

    let click = move |_| {
        copy_text(&text);
        *generation.write() += 1;
        let this_generation = *generation.peek();
        copied.set(true);
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            // Only revert if no newer copy happened in the meantime.
            if *generation.peek() == this_generation {
                copied.set(false);
            }
        });
    };

    let icon_class = tw_merge!("size-3.5", if copied() { "text-success" } else { "" });

    match children {
        Some(children) => rsx! {
            button {
                class: tw_merge!(base, class),
                onclick: click,
                aria_label: "Copy",
                {children}
                if copied() {
                    Check { class: icon_class }
                }
            }
        },
        None => rsx! {
            button {
                class: tw_merge!(base, class),
                onclick: click,
                aria_label: "Copy",
                if copied() {
                    Check { class: icon_class }
                } else {
                    Copy { class: icon_class }
                }
            }
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CopyButtonVariant {
    #[default]
    Ghost,
    Outline,
}
