use dioxus::prelude::*;
use dioxus_icons::lucide::{CircleCheck, CircleX, RefreshCw};

use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};

/// The Tools tab: availability of external tools used by preprocessing.
#[component]
pub fn ToolsTab() -> Element {
    // Probing runs `ffmpeg -version` / `ffprobe -version`, which blocks while
    // the processes start, so keep it off the UI thread.
    let mut tools = use_resource(|| async {
        tokio::task::spawn_blocking(upio::preprocess::tools::available_tools)
            .await
            .unwrap_or_default()
    });

    rsx! {
        div { class: "flex flex-col gap-5",
            div { class: "flex items-center justify-between",
                div { class: "flex flex-col gap-0.5",
                    h3 { class: "font-heading font-semibold text-lg", "External tools" }
                    p { class: "text-muted-foreground text-sm",
                        "Strategies that need a missing tool are skipped at upload time."
                    }
                }
                Button {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    onclick: move |_| tools.restart(),
                    RefreshCw {}
                    "Refresh"
                }
            }

            match &*tools.read() {
                Some(tools) => rsx! {
                    for (tool, available) in tools {
                        ToolCard {
                            key: "{tool.binary()}",
                            name: tool.binary(),
                            description: tool_description(*tool),
                            available: *available,
                        }
                    }
                },
                None => rsx! {
                    p { class: "text-sm text-muted-foreground", "Checking…" }
                },
            }
        }
    }
}

fn tool_description(tool: upio::preprocess::tools::Tool) -> &'static str {
    match tool {
        upio::preprocess::tools::Tool::Ffmpeg => {
            "Splits videos into playable segments for the \"Split video\" strategy."
        }
        upio::preprocess::tools::Tool::Ffprobe => {
            "Reads video metadata so segments can be sized correctly."
        }
    }
}

#[component]
fn ToolCard(name: String, description: &'static str, available: bool) -> Element {
    rsx! {
        Card {
            CardHeader {
                CardTitle { class: "font-mono text-base", "{name}" }
                CardDescription { "{description}" }
            }
            CardContent {
                if available {
                    Badge { variant: BadgeVariant::Success,
                        CircleCheck { class: "size-3" }
                        "Installed"
                    }
                } else {
                    Badge { variant: BadgeVariant::Error,
                        CircleX { class: "size-3" }
                        "Not found"
                    }
                }
            }
        }
    }
}
