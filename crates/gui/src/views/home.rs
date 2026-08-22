use std::path::PathBuf;

use dioxus::html::HasFileData;
use dioxus::prelude::*;
use dioxus_icons::lucide::{
    ChevronDown, ChevronRight, CircleAlert, CircleCheck, Clock, FolderOpen, Settings, Trash2,
    Upload,
};
use dioxus_primitives::ContentSide;
use tw_merge::*;
use upio::registry::UploaderId;

use crate::components::settings::set_settings_open;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::copy_button::CopyButton;
use crate::components::ui::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use crate::components::ui::field::{Field, FieldLabel};
use crate::components::ui::input::Input;
use crate::components::ui::progress::Progress;
use crate::components::ui::select;
use crate::components::ui::spinner::Spinner;
use crate::components::ui::table::{
    Table, TableBody, TableCell, TableHead, TableHeader, TableRow, TableVariant,
};
use crate::components::ui::toast::{use_toast, ToastOptions};
use crate::components::ui::tooltip::{Tooltip, TooltipContent, TooltipTrigger};
use crate::state::upload_queue::{use_queue, ItemStatus, OutcomeState, QueueItem, UploadQueue};
use crate::utils::{format_size, service_display_name};

/// The Home page: the upload workspace.
#[component]
pub fn Home() -> Element {
    rsx! {
        div {
            class: "mx-auto flex h-full w-full max-w-3xl flex-col gap-4 p-6",
            Header {}
            Dropzone {}
            ControlsRow {}
            FolderPicker {}
            QueueSection {}
        }
    }
}

#[component]
fn Header() -> Element {
    rsx! {
        div {
            class: "flex items-start justify-between gap-4",
            div {
                class: "flex flex-col gap-1",
                h1 { class: "font-heading font-semibold text-xl", "Upload files" }
                p { class: "text-muted-foreground text-sm",
                    "Drop files anywhere in this area to upload them."
                }
            }
            Tooltip {
                TooltipTrigger {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::IconSm,
                        onclick: move |_| set_settings_open(true),
                        aria_label: "Open settings",
                        Settings { class: "size-4" }
                    }
                }
                TooltipContent { side: ContentSide::Bottom, "Settings" }
            }
        }
    }
}

#[component]
fn Dropzone() -> Element {
    let mut queue = use_queue();
    let mut dragging = use_signal(|| false);

    let open_picker = |mut queue: UploadQueue| {
        spawn(async move {
            if let Some(files) = rfd::AsyncFileDialog::new().pick_files().await {
                let paths: Vec<PathBuf> =
                    files.iter().map(|file| file.path().to_path_buf()).collect();
                queue.enqueue(paths);
            }
        });
    };

    rsx! {
        div {
            class: tw_merge!(
                "flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed p-8 text-center transition-colors",
                if dragging() {
                    "border-primary bg-primary/4"
                } else {
                    "hover:bg-accent/32"
                },
            ),
            role: "button",
            tabindex: "0",
            aria_label: "Choose files to upload",
            onclick: move |_| open_picker(queue),
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Enter || e.key() == Key::Character(" ".to_string()) {
                    e.prevent_default();
                    open_picker(queue);
                }
            },
            ondragover: move |e| {
                e.prevent_default();
                dragging.set(true);
            },
            ondragleave: move |_| dragging.set(false),
            ondrop: move |e| {
                e.prevent_default();
                dragging.set(false);
                let paths: Vec<PathBuf> = e.files().iter().map(|f| f.path()).collect();
                queue.enqueue(paths);
            },
            Upload { class: "size-7 text-muted-foreground" }
            p {
                class: "font-medium text-sm",
                "Drop files or click to browse"
            }
        }
    }
}

#[component]
fn ControlsRow() -> Element {
    rsx! {
        div {
            class: "grid grid-cols-2 gap-x-6 gap-y-3",
            ServicePicker {}
            ConcurrencyPicker {}
        }
    }
}

#[component]
fn FolderPicker() -> Element {
    let mut queue = use_queue();

    rsx! {
        Field {
            class: "w-full",
            FieldLabel {
                html_for: "upload-folder",
                class: "text-muted-foreground text-xs",
                "Folder"
            }
            Input {
                id: "upload-folder",
                placeholder: "created if missing (optional)",
                autocomplete: "off",
                value: "{queue.folder_name.read()}",
                oninput: move |e: FormEvent| queue.folder_name.set(e.value()),
            }
        }
    }
}

#[component]
fn ServicePicker() -> Element {
    let mut queue = use_queue();
    let toast = use_toast();
    let mut selected_names =
        use_signal(|| Some(names_of(&queue.services.peek())) as Option<Vec<&'static str>>);
    let mut placeholder = use_signal(move || joined_names(&queue.services.peek()));

    rsx! {
        Field {
            class: "gap-1.5",
            FieldLabel { html_for: "upload-services", class: "text-muted-foreground text-xs", "Services" }
            select::SelectMulti::<&'static str> {
                id: "upload-services",
                class: "w-full",
                values: ReadSignal::new(selected_names),
                default_values: names_of(&queue.services.peek()),
                placeholder: ReadSignal::new(placeholder),
                on_values_change: move |values: Vec<&'static str>| {
                    let ids: Vec<UploaderId> = values
                        .iter()
                        .filter_map(|name| UploaderId::from_name(name))
                        .collect();
                    if queue.set_services(ids.clone()) {
                        selected_names.set(Some(values));
                        placeholder.set(joined_names(&ids));
                    } else {
                        toast.warning(
                            "Busy".to_string(),
                            ToastOptions::new()
                                .description("Wait for the current uploads to finish before changing services."),
                        );
                    }
                },
                select::SelectGroup {
                    for known in UploaderId::ALL {
                        select::SelectOption::<&'static str> {
                            key: "{known.name()}",
                            index: index_of(known),
                            value: known.name(),
                            text_value: service_display_name(*known),
                            "{service_display_name(*known)}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ConcurrencyPicker() -> Element {
    let mut queue = use_queue();

    // Derive both the controlled value and its display label directly from
    // the owning signal; there is no duplicated writable state to go stale.
    let selected = use_memo(move || Some(batch_label(*queue.batch_size.read())));
    let label = use_memo(move || batch_label(*queue.batch_size.read()).to_string());

    rsx! {
        Field {
            class: "gap-1.5",
            FieldLabel { html_for: "upload-batch", class: "text-muted-foreground text-xs", "Concurrency" }
            select::Select::<&'static str> {
                id: "upload-batch",
                class: "w-full",
                value: ReadSignal::new(selected),
                placeholder: ReadSignal::new(label),
                on_value_change: move |value: Option<&'static str>| {
                    if let Some(value) = value {
                        queue.batch_size.set(batch_value(value));
                    }
                },
                select::SelectGroup {
                    select::SelectOption::<&'static str> { index: 0usize, value: "Sequential", "Sequential" }
                    select::SelectOption::<&'static str> { index: 1usize, value: "2 at a time", "2 at a time" }
                    select::SelectOption::<&'static str> { index: 2usize, value: "4 at a time", "4 at a time" }
                    select::SelectOption::<&'static str> { index: 3usize, value: "8 at a time", "8 at a time" }
                }
            }
        }
    }
}

fn names_of(services: &[UploaderId]) -> Vec<&'static str> {
    services.iter().map(|id| id.name()).collect()
}

fn joined_names(services: &[UploaderId]) -> String {
    if services.is_empty() {
        "No services".to_string()
    } else {
        services
            .iter()
            .map(|id| service_display_name(*id))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn index_of(id: &UploaderId) -> usize {
    UploaderId::ALL
        .iter()
        .position(|known| known == id)
        .unwrap_or(0)
}

fn batch_label(size: usize) -> &'static str {
    match size {
        0..=1 => "Sequential",
        2 => "2 at a time",
        3..=4 => "4 at a time",
        _ => "8 at a time",
    }
}

fn batch_value(label: &str) -> usize {
    match label {
        "2 at a time" => 2,
        "4 at a time" => 4,
        "8 at a time" => 8,
        _ => 1,
    }
}

#[component]
fn QueueSection() -> Element {
    let queue = use_queue();
    let is_empty = queue.items.read().is_empty();

    if is_empty {
        rsx! {
            div {
                class: "flex min-h-0 flex-1 items-center justify-center",
                Empty {
                    EmptyHeader {
                        EmptyMedia {
                            variant: EmptyMediaVariant::Icon,
                            Upload {}
                        }
                        EmptyTitle { "Nothing queued yet" }
                        EmptyDescription {
                            "Dropped files wait here for your confirmation before uploading."
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "flex min-h-0 flex-1 flex-col gap-3",
                ConfirmBar {}
                div {
                    class: "min-h-0 flex-1 overflow-y-auto pb-1",
                    QueueTable {}
                }
                SummaryBar {}
            }
        }
    }
}

/// Shown whenever files are waiting for confirmation.
#[component]
fn ConfirmBar() -> Element {
    let mut queue = use_queue();

    let pending_count = queue
        .items
        .read()
        .iter()
        .filter(|item| item.status == ItemStatus::Pending)
        .count();
    if pending_count == 0 {
        return rsx! {};
    }

    let services_text = joined_names(&queue.services.read());
    let message = format!(
        "{} file{} ready to upload to {}",
        pending_count,
        if pending_count == 1 { "" } else { "s" },
        services_text
    );

    rsx! {
        div {
            class: "flex flex-wrap items-center justify-between gap-3 rounded-xl border border-primary/32 bg-primary/4 px-3.5 py-2.5 text-sm",
            span { "{message}" }
            div {
                class: "flex items-center gap-2",
                Button {
                    size: ButtonSize::Sm,
                    onclick: move |_| queue.confirm(),
                    Upload {}
                    "Upload"
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    onclick: move |_| {
                        let ids: Vec<u64> = queue
                            .items
                            .read()
                            .iter()
                            .filter(|item| item.status == ItemStatus::Pending)
                            .map(|item| item.id)
                            .collect();
                        for id in ids {
                            queue.discard(id);
                        }
                    },
                    Trash2 {}
                    "Discard"
                }
            }
        }
    }
}

#[component]
fn QueueTable() -> Element {
    let queue = use_queue();
    let items = queue.items.read();

    rsx! {
        Table {
            variant: TableVariant::Card,
            TableHeader {
                TableRow {
                    TableHead { class: "w-6", "" }
                    TableHead { "File" }
                    TableHead { class: "w-20", "Size" }
                    TableHead { class: "w-44", "Status" }
                    TableHead { class: "w-10", "" }
                }
            }
            TableBody {
                for item in items.iter() {
                    QueueRow { key: "{item.id}", item: item.clone() }
                }
            }
        }
    }
}

#[component]
fn QueueRow(item: QueueItem) -> Element {
    let mut expanded = use_signal(|| false);
    let name = item
        .file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| item.file.display().to_string());
    let is_pending = item.status == ItemStatus::Pending;

    rsx! {
        TableRow {
            TableCell { class: "w-6 pl-3",
                button {
                    class: "rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground",
                    aria_label: if expanded() { "Hide details" } else { "Show details" },
                    onclick: move |_| expanded.toggle(),
                    if expanded() {
                        ChevronDown { class: "size-4" }
                    } else {
                        ChevronRight { class: "size-4" }
                    }
                }
            }
            TableCell {
                class: "max-w-56 font-medium",
                div { class: "truncate", "{name}" }
            }
            TableCell { class: "text-muted-foreground", "{format_size(upio::filesize::FileSize::from_bytes(item.size))}" }
            TableCell { StatusCell { item: item.clone() } }
            TableCell {
                class: "text-right",
                if is_pending {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::IconSm,
                        aria_label: "Discard file",
                        onclick: move |_| {
                            let id = item.id;
                            use_queue().discard(id);
                        },
                        Trash2 { class: "size-3.5" }
                    }
                }
            }
        }
        if expanded() {
            tr {
                td {
                    colspan: 5,
                    class: "border-none bg-muted/24 p-3",
                    OutcomeDetail { item: item.clone() }
                }
            }
        }
    }
}

#[component]
fn StatusCell(item: QueueItem) -> Element {
    match item.status {
        ItemStatus::Pending => rsx! {
            span {
                class: "inline-flex items-center gap-1.5 text-muted-foreground text-sm",
                Clock { class: "size-4" }
                "Ready"
            }
        },
        ItemStatus::Working => {
            let progress = item.progress();
            let percent = progress.and_then(|(uploaded, total)| {
                (total > 0).then(|| (uploaded as f64 / total as f64 * 100.0) as u64)
            });
            let label = match percent {
                Some(percent) => format!("Uploading... {percent}%"),
                None => "Uploading".to_string(),
            };
            rsx! {
                div {
                    class: "flex flex-col gap-1",
                    span {
                        class: "inline-flex items-center gap-1.5 text-muted-foreground text-sm",
                        Spinner { class: "size-3.5" }
                        "{label}"
                    }
                    if let Some(percent) = percent {
                        Progress {
                            value: Some(percent as f64),
                            max: 100.0,
                            class: "h-1 w-32",
                        }
                    }
                }
            }
        }
        ItemStatus::Done => rsx! {
            span {
                class: "inline-flex items-center gap-1.5 text-success text-sm",
                CircleCheck { class: "size-4" }
                "Done"
            }
        },
        ItemStatus::Failed => rsx! {
            span {
                class: "inline-flex items-center gap-1.5 text-destructive text-sm",
                CircleAlert { class: "size-4" }
                "Failed"
            }
        },
    }
}

/// Per-service detail for one queued file.
#[component]
fn OutcomeDetail(item: QueueItem) -> Element {
    let queue = use_queue();
    let outcomes = item.outcomes.clone();

    rsx! {
        div {
            class: "flex flex-col gap-1 text-sm",
            div {
                class: "grid grid-cols-[7rem_1fr] items-baseline gap-y-1",
                span { class: "text-muted-foreground", "Target folder" }
                span {
                    class: "font-mono text-xs",
                    "{queue.folder_name.read().trim().to_string()}"
                }
            }

            for outcome in outcomes {
                div {
                    key: "{outcome.service.name()}",
                    class: "rounded-lg border bg-card p-3 not-dark:bg-clip-padding",
                    div {
                        class: "flex items-center justify-between gap-2",
                        div {
                            class: "flex items-center gap-2 font-medium",
                            "{service_display_name(outcome.service)}"
                            match outcome.state {
                                OutcomeState::Waiting => rsx! {
                                    Badge { variant: BadgeVariant::Secondary, "Waiting" }
                                },
                                OutcomeState::Uploading => rsx! {
                                    Badge { variant: BadgeVariant::Info, "Uploading" }
                                },
                                OutcomeState::Done => rsx! {
                                    Badge { variant: BadgeVariant::Success, "Done" }
                                },
                                OutcomeState::Failed => rsx! {
                                    Badge { variant: BadgeVariant::Error, "Failed" }
                                },
                                OutcomeState::Cancelled => rsx! {
                                    Badge { variant: BadgeVariant::Secondary, "Stopped" }
                                },
                            }
                        }
                        if outcome.state == OutcomeState::Uploading {
                            if let Some((uploaded, total)) =
                                outcome.progress.filter(|(_, total)| *total > 0)
                            {
                                span {
                                    class: "font-mono text-muted-foreground text-xs",
                                    "{format_size(upio::filesize::FileSize::from_bytes(uploaded))} / {format_size(upio::filesize::FileSize::from_bytes(total))}"
                                }
                            }
                        }
                    }

                    if let Some(error) = &outcome.error {
                        p {
                            class: "flex items-start gap-1.5 text-destructive",
                            CircleAlert { class: "mt-0.5 size-3.5 shrink-0" }
                            "{error}"
                        }
                    } else if !outcome.urls.is_empty() {
                        ul {
                            class: "flex flex-col gap-1",
                            for url in outcome.urls.clone() {
                                li {
                                    key: "{url}",
                                    class: "flex min-w-0 items-center gap-1.5",
                                    a {
                                        class: "min-w-0 flex-1 truncate text-muted-foreground hover:underline",
                                        href: "{url}",
                                        target: "_blank",
                                        "{url}"
                                    }
                                    CopyButton { text: url.clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SummaryBar() -> Element {
    let mut queue = use_queue();

    let (done, failed, running) = {
        let items = queue.items.read();
        let done = items
            .iter()
            .filter(|item| item.status == ItemStatus::Done)
            .count();
        let failed = items
            .iter()
            .filter(|item| item.status == ItemStatus::Failed)
            .count();
        let running = *queue.running.peek();
        (done, failed, running)
    };

    rsx! {
        div {
            class: "flex flex-wrap items-center justify-between gap-3 rounded-xl border bg-card px-3.5 py-2.5 not-dark:bg-clip-padding text-sm",
            div {
                class: "flex items-center gap-4 text-muted-foreground",
                if done > 0 {
                    span { class: "text-success", "{done} uploaded" }
                }
                if failed > 0 {
                    span { class: "text-destructive", "{failed} failed" }
                }
                if done + failed == 0 {
                    span { "Empty" }
                }
            }
            div {
                class: "flex items-center gap-2",
                if running {
                    Button {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        onclick: move |_| queue.request_stop(),
                        "Stop after current upload"
                    }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    disabled: !running && done + failed == 0,
                    onclick: move |_| {
                        if !queue.clear_all() {
                            // Running; fall back to clearing only finished rows.
                            queue.clear_finished();
                        }
                    },
                    FolderOpen {}
                    "Clear"
                }
            }
        }
    }
}
