use dioxus::prelude::*;
use dioxus_icons::lucide::{Copy, ExternalLink, Info, RefreshCw};

use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::config_store::use_config;
use crate::utils::{copy_text, reveal_path};
use upio_config::Config;

/// The Config file tab: a read-only view of the effective config file.
#[component]
pub fn ConfigFileTab() -> Element {
    let store = use_config();
    let store_for_copy = store;
    let mut store_for_reload = store;
    let path = Config::path();

    rsx! {
        div { class: "flex flex-col gap-4",
            div { class: "flex flex-wrap items-center justify-between gap-3",
                div { class: "flex min-w-0 flex-col",
                    span { class: "text-sm font-medium", "{path.display()}" }
                    span { class: "text-muted-foreground text-xs",
                        "UPIO_* environment variables take precedence over this file."
                    }
                }
                div { class: "flex items-center gap-1.5",
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::IconSm,
                        onclick: move |_| store_for_reload.reload(),
                        RefreshCw {}
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::IconSm,
                        onclick: move |_| copy_text(&store_for_copy.raw_toml()),
                        Copy {}
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::IconSm,
                        onclick: move |_| reveal_path(&path),
                        ExternalLink {}
                    }
                }
            }

            div { class: "flex items-start gap-2 rounded-lg border border-info/30 bg-info/8 p-3 text-sm text-muted-foreground",
                Info { class: "mt-0.5 size-4 shrink-0 text-info" }
                span {
                    "This preview updates as you change settings. Edits made outside the app "
                    "appear after you press reload."
                }
            }

            pre { class: "max-h-80 overflow-auto rounded-xl border bg-card p-4 not-dark:bg-clip-padding font-mono text-xs leading-relaxed",
                code { "{store.raw_toml()}" }
            }
        }
    }
}
