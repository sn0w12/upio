use rune::{col, el, icon, row, spacer, text, Color, El, FileDialogOptions, ScrollSpec, Tx};
use rune_kit::glyphs;
use rune_kit::{
    Button, ButtonVariant, Checkbox, ControlSize, IconButton, Kit, Popover, Select,
    SelectAppearance, TextInput,
};
use upio::filesize::FileSize;

use crate::app::{App, ItemStatus, OutcomeState, Page, QueueItem, ServiceOutcome};

use super::{copy_button, notice, page_header};

const CONCURRENCY: [&str; 4] = ["Sequential", "2 at a time", "4 at a time", "8 at a time"];

pub fn view(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    let mut page = col()
        .key("upload-page")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .px(18.0)
        .py(14.0)
        .gap(10.0)
        .child(
            row()
                .key("upload-header")
                .w_full()
                .justify_between()
                .center_y()
                .gap(12.0)
                .child(page_header(
                    k,
                    "Upload",
                    "Drop files, pick destinations, then review the queue.",
                ))
                .child(
                    Button::new("Settings")
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Small)
                        .on_press(|s: &mut App| s.set_page(Page::Settings))
                        .build(k),
                ),
        );

    if let Some(notice) = notice(s, k) {
        page = page.child(notice);
    }

    if s.queue.is_empty() {
        page.child(destinations(s, k, layer))
            .child(empty_dropzone(s, k))
    } else {
        page.child(destinations(s, k, layer))
            .child(dropzone(s, k))
            .child(queue(s, k))
    }
}

fn empty_dropzone(s: &App, k: &Kit) -> El {
    let hovering = s.drop_hover;
    col()
        .key("dropzone")
        .w_full()
        .grow(1.0)
        .min_h(140.0)
        .center()
        .gap(6.0)
        .radius(k.radius.lg)
        .fill(if hovering { k.c.layer_02 } else { k.c.layer_01 })
        .hover(k.c.layer_02)
        .border(
            if hovering { 1.5 } else { 1.0 },
            if hovering {
                k.c.border_focus
            } else {
                k.c.border_base
            },
        )
        .role("button")
        .label("Add files")
        .focusable(true)
        .on_press_tx(|_: &mut App, tx: &mut Tx| open_file(tx))
        .on_drop_files(|s: &mut App, files, _| {
            s.drop_hover = false;
            s.add_files(files.to_vec());
        })
        .on_drop_hover(|s: &mut App, over, tx| {
            s.drop_hover = over;
            tx.live_only();
        })
        .child(
            el().size(44.0, 44.0)
                .center()
                .radius(22.0)
                .fill(k.c.layer_03)
                .child(icon(&glyphs::FILE, 20.0).color(k.c.icon_muted)),
        )
        .child(
            text("Drop files or folders here")
                .font(k.ty.body_medium)
                .color(k.c.text_base)
                .mt(4.0),
        )
        .child(
            text("or click to pick a file")
                .font(k.ty.caption())
                .color(k.c.text_muted),
        )
}

fn dropzone(s: &App, k: &Kit) -> El {
    let hovering = s.drop_hover;
    row()
        .key("dropzone")
        .w_full()
        .h(60.0)
        .padding(12.0)
        .gap(10.0)
        .center_y()
        .radius(k.radius.lg)
        .fill(if hovering { k.c.layer_02 } else { k.c.layer_01 })
        .hover(k.c.layer_02)
        .border(
            if hovering { 1.5 } else { 1.0 },
            if hovering {
                k.c.border_focus
            } else {
                k.c.border_base
            },
        )
        .role("button")
        .label("Add files")
        .focusable(true)
        .on_press_tx(|_: &mut App, tx: &mut Tx| open_file(tx))
        .on_drop_files(|s: &mut App, files, _| {
            s.drop_hover = false;
            s.add_files(files.to_vec());
        })
        .on_drop_hover(|s: &mut App, over, tx| {
            s.drop_hover = over;
            tx.live_only();
        })
        .child(
            el().size(30.0, 30.0)
                .center()
                .radius(15.0)
                .fill(k.c.layer_03)
                .child(icon(&glyphs::FILE, 15.0).color(k.c.icon_muted)),
        )
        .child(
            col()
                .gap(1.0)
                .child(
                    text("Drop files or folders here")
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text("or click to pick a file")
                        .font(k.ty.caption())
                        .color(k.c.text_muted),
                ),
        )
}

fn destinations(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    let enabled = s.active_services();
    let summary = if enabled.is_empty() {
        "No services".to_string()
    } else if s.services.len() == enabled.len() {
        format!("{} services", enabled.len())
    } else {
        format!("{} of {} services", s.services.len(), enabled.len())
    };

    let trigger = Button::new(summary)
        .variant(ButtonVariant::Outline)
        .control_size(ControlSize::Small)
        .on_press(|s: &mut App| s.toggle_open("destinations"))
        .build(k);

    let mut body = col().gap(2.0);
    if enabled.is_empty() {
        body = body.child(
            text("No services enabled. Enable one in Settings.")
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        );
    } else {
        body = body.child(
            row()
                .w_full()
                .justify_between()
                .center_y()
                .pb(4.0)
                .child(text("Services").font(k.ty.caption()).color(k.c.text_muted))
                .child(
                    row()
                        .gap(2.0)
                        .child(
                            Button::new("All")
                                .variant(ButtonVariant::Ghost)
                                .control_size(ControlSize::Mini)
                                .on_press(move |s: &mut App| {
                                    s.set_services(s.active_services());
                                })
                                .build(k),
                        )
                        .child(
                            Button::new("None")
                                .variant(ButtonVariant::Ghost)
                                .control_size(ControlSize::Mini)
                                .on_press(|s: &mut App| s.set_services(Vec::new()))
                                .build(k),
                        ),
                ),
        );
        for id in &enabled {
            let id = *id;
            body = body.child(
                Checkbox::new(s.selected(id))
                    .label(id.def().display_name)
                    .disabled(s.running)
                    .on_toggle(move |s: &mut App| s.toggle_service(id))
                    .build(k),
            );
        }
    }

    let picker = Popover::new("destinations")
        .trigger(trigger)
        .open(s.is_open("destinations"))
        .on_close(|s: &mut App| s.close_open())
        .label("Send to")
        .child(body)
        .build(k, layer);

    row()
        .key("destinations")
        .w_full()
        .gap(10.0)
        .center_y()
        .child(
            text("Send to")
                .font(k.ty.small_medium)
                .color(k.c.text_muted),
        )
        .child(picker)
        .child(spacer())
        .child(
            TextInput::new(&s.folder)
                .label("Folder")
                .placeholder("Folder (optional)")
                .width(Some(180.0))
                .on_edit(|s: &mut App| &mut s.folder)
                .build(k),
        )
        .child(
            Select::new("concurrency", &CONCURRENCY)
                .appearance(SelectAppearance::Field)
                .width(130.0)
                .label("Concurrency")
                .current(Some(concurrency_label(s.concurrency)))
                .open(s.is_open("concurrency"))
                .on_toggle(|s: &mut App| s.toggle_open("concurrency"))
                .on_close(|s: &mut App| s.close_open())
                .on_choose(|s: &mut App, value, _| {
                    s.set_concurrency(concurrency_value(value));
                    s.close_open();
                })
                .build(k, layer),
        )
}

fn queue(s: &App, k: &Kit) -> El {
    let pending = count(s, ItemStatus::Pending);

    let mut list = col()
        .key("queue-list")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });

    for (index, item) in s.queue.iter().enumerate() {
        if index > 0 {
            list = list.child(el().w_full().h(1.0).fill(k.c.border_muted).a11y_hidden());
        }
        list = list.child(item_row(s, k, item));
    }

    let mut actions = row().gap(4.0).center_y();
    if s.running {
        actions = actions.child(
            Button::new(if s.stopping { "Stopping" } else { "Stop" })
                .variant(ButtonVariant::Danger)
                .control_size(ControlSize::Small)
                .disabled(s.stopping)
                .on_press(|s: &mut App| s.request_stop())
                .build(k),
        );
    } else {
        actions = actions.child(
            Button::new("Clear finished")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .disabled(
                    s.queue
                        .iter()
                        .all(|item| item.status == ItemStatus::Pending),
                )
                .on_press(|s: &mut App| s.clear_finished())
                .build(k),
        );
    }
    actions = actions.child(
        Button::new("Upload")
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .disabled(s.running || pending == 0 || s.services.is_empty())
            .on_press(|s: &mut App| s.confirm())
            .build(k),
    );

    col()
        .key("queue")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .radius(k.radius.lg)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .child(
            row()
                .key("queue-head")
                .w_full()
                .px(10.0)
                .py(7.0)
                .gap(8.0)
                .center_y()
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_muted)
                .child(text("Queue").font(k.ty.ui_medium).color(k.c.text_base))
                .child(text(summary(s)).font(k.ty.caption()).color(k.c.text_faint))
                .child(spacer())
                .child(actions),
        )
        .child(list)
}

fn item_row(s: &App, k: &Kit, item: &QueueItem) -> El {
    let key = format!("queue-{}", item.id);
    let expanded = s.is_open(&key);
    let id = item.id;

    let mut head = row()
        .key("item-head")
        .w_full()
        .px(10.0)
        .py(8.0)
        .gap(8.0)
        .center_y()
        .child(icon(&glyphs::FILE, 15.0).color(k.c.icon_muted))
        .child(
            text(item.file_name())
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .grow(1.0)
                .shrink()
                .truncate(),
        );
    if item.status == ItemStatus::Working {
        if let Some((uploaded, total)) = item.progress() {
            let percent = if total > 0 {
                (uploaded as f64 / total as f64 * 100.0) as u32
            } else {
                0
            };
            head = head.child(
                text(format!("{percent}%"))
                    .font(k.ty.caption())
                    .color(k.c.text_muted),
            );
        }
    }
    head = head
        .child(
            text(format_size(item.size))
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
        .child(outcome_dots(item, k));

    head = head.child(
        IconButton::new(if expanded {
            k.glyph.chevron_up
        } else {
            k.glyph.chevron_down
        })
        .label("Upload details")
        .on_press(move |s: &mut App| s.toggle_open(&key))
        .build(k),
    );

    let mut block = col()
        .key(format!("item-{}", item.id))
        .w_full()
        .fill(if expanded {
            k.c.layer_02
        } else {
            Color::TRANSPARENT
        })
        .child(head);
    if expanded {
        let mut details = col()
            .key("details")
            .w_full()
            .gap(1.0)
            .px(33.0)
            .pt(6.0)
            .pb(10.0);
        for outcome in &item.outcomes {
            details = details.child(outcome_lines(outcome, k));
        }
        if item.status == ItemStatus::Pending {
            details = details.child(
                row().pt(4.0).child(
                    Button::new("Remove from queue")
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Mini)
                        .on_press(move |s: &mut App| s.discard(id))
                        .build(k),
                ),
            );
        }
        block = block
            .child(
                row()
                    .w_full()
                    .pl(33.0)
                    .child(el().grow(1.0).h(1.0).fill(k.c.border_base)),
            )
            .child(details);
    }
    block
}

fn outcome_lines(outcome: &ServiceOutcome, k: &Kit) -> El {
    let (state, color) = match outcome.state {
        OutcomeState::Waiting => ("queued".to_string(), k.c.text_faint),
        OutcomeState::Uploading => (
            outcome
                .progress
                .map(|(uploaded, total)| {
                    if total > 0 {
                        format!("{:.0}%", uploaded as f64 / total as f64 * 100.0)
                    } else {
                        "uploading".to_string()
                    }
                })
                .unwrap_or_else(|| "uploading".to_string()),
            k.c.info.fg,
        ),
        OutcomeState::Done => ("uploaded".to_string(), k.c.success.fg),
        OutcomeState::Failed => ("failed".to_string(), k.c.danger.fg),
        OutcomeState::Cancelled => ("cancelled".to_string(), k.c.text_faint),
    };

    let labels = |first: bool, k: &Kit| {
        row()
            .gap(8.0)
            .h(20.0)
            .center_y()
            .child(
                text(if first {
                    outcome.service.def().display_name.to_string()
                } else {
                    String::new()
                })
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .w(86.0),
            )
            .child(
                text(if first { state.clone() } else { String::new() })
                    .font(k.ty.caption())
                    .color(color)
                    .w(58.0),
            )
    };

    let mut lines = col()
        .key(format!("outcome-{}", outcome.service.name()))
        .w_full()
        .gap(1.0);

    if matches!(outcome.state, OutcomeState::Done) {
        for (index, url) in outcome.urls.iter().enumerate() {
            lines = lines.child(
                labels(index == 0, k)
                    .grow(1.0)
                    .child(
                        text(url.clone())
                            .font(k.ty.caption())
                            .color(k.c.text_muted)
                            .selectable(true)
                            .grow(1.0)
                            .shrink()
                            .truncate(),
                    )
                    .child(copy_button(k, url.clone(), "Copy link")),
            );
        }
        if outcome.urls.is_empty() {
            lines = lines.child(labels(true, k));
        }
    } else if let Some(error) = &outcome.error {
        lines = lines.child(
            labels(true, k).grow(1.0).child(
                text(error.clone())
                    .font(k.ty.caption())
                    .color(k.c.danger.fg)
                    .grow(1.0)
                    .shrink()
                    .truncate(),
            ),
        );
    } else {
        lines = lines.child(labels(true, k));
    }
    lines
}

fn outcome_dots(item: &QueueItem, k: &Kit) -> El {
    let mut dots = row().key("outcome-dots").gap(3.0).center_y();
    for outcome in &item.outcomes {
        let color = match outcome.state {
            OutcomeState::Waiting => k.c.border_strong,
            OutcomeState::Uploading => k.c.info.fg,
            OutcomeState::Done => k.c.success.fg,
            OutcomeState::Failed => k.c.danger.fg,
            OutcomeState::Cancelled => k.c.text_faint,
        };
        dots = dots.child(el().size(5.0, 5.0).radius(2.5).fill(color));
    }
    dots
}

fn summary(s: &App) -> String {
    let mut parts = Vec::new();
    for (status, label) in [
        (ItemStatus::Pending, "pending"),
        (ItemStatus::Working, "uploading"),
        (ItemStatus::Done, "done"),
        (ItemStatus::Failed, "failed"),
    ] {
        let n = count(s, status);
        if n > 0 {
            parts.push(format!("{n} {label}"));
        }
    }
    if parts.is_empty() {
        "empty".to_string()
    } else {
        parts.join(" · ")
    }
}

fn count(s: &App, status: ItemStatus) -> usize {
    s.queue.iter().filter(|item| item.status == status).count()
}

fn format_size(bytes: u64) -> String {
    FileSize::from_bytes(bytes).to_human_string()
}

fn concurrency_label(value: usize) -> &'static str {
    match value {
        0 | 1 => "Sequential",
        2 => "2 at a time",
        3 | 4 => "4 at a time",
        _ => "8 at a time",
    }
}

fn concurrency_value(label: &str) -> usize {
    match label {
        "2 at a time" => 2,
        "4 at a time" => 4,
        "8 at a time" => 8,
        _ => 1,
    }
}

fn open_file(tx: &mut Tx) {
    tx.open_file(
        FileDialogOptions {
            title: "Add files".to_string(),
            ..Default::default()
        },
        |s: &mut App, result, _| {
            if let Ok(Some(path)) = result {
                s.add_files(vec![path]);
            }
        },
    );
}
