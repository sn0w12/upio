use std::path::Path;

use rune::{col, el, icon, row, spacer, text, El, ScrollSpec};
use rune_kit::glyphs;
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, Checkbox, ControlSize, IconButton, Kit, Select,
    SelectAppearance, Switch, TextArea, TextInput,
};
use upio::filesize::FileSize;
use upio::preprocess::tools::Tool;
use upio::preprocess::STRATEGIES;
use upio::registry::UploaderId;
use upio_config::{Config, ConfigKey};

use crate::app::{App, GuiPrefs, Page, SettingsSection};
use crate::worker::{self, Command};

use super::{copy_button, notice, page_header, theme_picker};

struct FieldSpec {
    key: ConfigKey,
    label: &'static str,
    placeholder: &'static str,
    help: &'static str,
    secret: bool,
}

static BUNKR_FIELDS: &[FieldSpec] = &[FieldSpec {
    key: ConfigKey::BunkrToken,
    label: "API token",
    placeholder: "Paste your Bunkr token",
    help: "Required for uploads. Create one in your Bunkr account settings.",
    secret: true,
}];

static GOFILE_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        key: ConfigKey::GofileToken,
        label: "Account token",
        placeholder: "Optional account token",
        help: "Optional. Raises limits and unlocks folders.",
        secret: true,
    },
    FieldSpec {
        key: ConfigKey::GofileServer,
        label: "Server",
        placeholder: "Auto",
        help: "Pin a server such as store4, or leave empty to choose automatically.",
        secret: false,
    },
    FieldSpec {
        key: ConfigKey::GofileLinkPassword,
        label: "Download link password",
        placeholder: "Folder password",
        help: "Unlocks password-protected folders. A ?password= on a link takes precedence.",
        secret: true,
    },
];

static FILESTER_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        key: ConfigKey::FilesterToken,
        label: "Account token",
        placeholder: "Optional account token",
        help: "Optional. Used for authenticated API access.",
        secret: true,
    },
    FieldSpec {
        key: ConfigKey::FilesterLinkPassword,
        label: "Download link password",
        placeholder: "Folder password",
        help: "Unlocks password-protected folders. A ?password= on a link takes precedence.",
        secret: true,
    },
];

static GOONBOX_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        key: ConfigKey::GoonboxUsername,
        label: "Username",
        placeholder: "GoonBox username",
        help: "Required for uploads.",
        secret: false,
    },
    FieldSpec {
        key: ConfigKey::GoonboxPassword,
        label: "Password",
        placeholder: "GoonBox password",
        help: "Required for uploads. The captcha is solved automatically.",
        secret: true,
    },
    FieldSpec {
        key: ConfigKey::GoonboxToken,
        label: "API key",
        placeholder: "Optional read-only API key",
        help: "Optional. Reads link-only albums and images for downloads.",
        secret: true,
    },
];

static PIXELDRAIN_FIELDS: &[FieldSpec] = &[FieldSpec {
    key: ConfigKey::PixeldrainToken,
    label: "API key",
    placeholder: "Paste your Pixeldrain API key",
    help: "Required for uploads. Create one on the Pixeldrain API keys page.",
    secret: true,
}];

fn fields(id: UploaderId) -> &'static [FieldSpec] {
    match id.name() {
        "bunkr" => BUNKR_FIELDS,
        "gofile" => GOFILE_FIELDS,
        "fileditch" => &[],
        "filester" => FILESTER_FIELDS,
        "goonbox" => GOONBOX_FIELDS,
        "pixeldrain" => PIXELDRAIN_FIELDS,
        _ => &[],
    }
}

pub fn view(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    let mut page = col()
        .key("settings-page")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .min_h(0.0)
        .px(18.0)
        .py(14.0)
        .gap(10.0)
        .child(settings_header(s, k));

    if let Some(load_error) = &s.load_error {
        page = page.child(
            row()
                .w_full()
                .gap(10.0)
                .center_y()
                .padding(10.0)
                .radius(k.radius.md)
                .fill(k.c.danger.bg)
                .border(1.0, k.c.danger.border)
                .child(
                    text(format!("Could not read config.toml: {load_error}"))
                        .font(k.ty.small)
                        .color(k.c.danger.fg)
                        .grow(1.0)
                        .shrink()
                        .truncate(),
                )
                .child(
                    Button::new("Reveal")
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Mini)
                        .on_press(|_: &mut App| reveal_path(&Config::path()))
                        .build(k),
                )
                .child(
                    Button::new("Reload")
                        .variant(ButtonVariant::Outline)
                        .control_size(ControlSize::Mini)
                        .on_press(|s: &mut App| s.reload_config())
                        .build(k),
                ),
        );
    }

    if let Some(notice) = notice(s, k) {
        page = page.child(notice);
    }

    let content = match s.settings_section {
        SettingsSection::Appearance => appearance(s, k),
        SettingsSection::Services => services(s, k),
        SettingsSection::Preprocessing => preprocessing(s, k, layer),
        SettingsSection::Tools => tools(s, k),
        SettingsSection::Advanced => advanced(s, k),
    };

    page.child(
        row()
            .key("settings-body")
            .grow(1.0)
            .min_h(0.0)
            .gap(14.0)
            .items_start()
            .child(settings_nav(s, k))
            .child(
                col()
                    .key("settings-content")
                    .grow(1.0)
                    .min_w(0.0)
                    .min_h(0.0)
                    .h_full()
                    .border_sides([0.0, 0.0, 0.0, 1.0], k.c.border_muted)
                    .pl(18.0)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .child(content),
            ),
    )
}

fn settings_header(s: &App, k: &Kit) -> El {
    let mut status = row().gap(8.0).center_y();
    if let Some(error) = &s.save_error {
        status = status.child(
            text(error.clone())
                .font(k.ty.caption())
                .color(k.c.danger.fg)
                .w(260.0)
                .truncate(),
        );
    } else if s.dirty {
        status = status.child(
            text("Unsaved changes")
                .font(k.ty.caption())
                .color(k.c.warning.fg),
        );
    }

    row()
        .key("settings-header")
        .w_full()
        .center_y()
        .gap(10.0)
        .child(page_header(
            k,
            "Settings",
            "Appearance, services, preprocessing, and the advanced config file.",
        ))
        .child(spacer())
        .child(
            row()
                .gap(6.0)
                .center_y()
                .child(status)
                .child(
                    Button::new("Back")
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Small)
                        .on_press(|s: &mut App| s.set_page(Page::Upload))
                        .build(k),
                )
                .child(
                    Button::new("Discard")
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Small)
                        .disabled(!s.dirty)
                        .on_press(|s: &mut App| s.discard_changes())
                        .build(k),
                )
                .child(
                    Button::new("Save changes")
                        .variant(ButtonVariant::Contrast)
                        .control_size(ControlSize::Small)
                        .disabled(!s.dirty || s.load_error.is_some())
                        .on_press(|s: &mut App| s.save_config())
                        .build(k),
                ),
        )
}

fn appearance(s: &App, k: &Kit) -> El {
    col().key("appearance-section").w_full().gap(14.0).child(
        col()
            .gap(4.0)
            .child(text("Theme").font(k.ty.ui_medium).color(k.c.text_base))
            .child(
                text("Follow the system appearance, or pin light or dark.")
                    .font(k.ty.caption())
                    .color(k.c.text_muted),
            )
            .child(theme_picker(s, k)),
    )
}

fn settings_nav(s: &App, k: &Kit) -> El {
    let mut nav = col().key("settings-nav").w(150.0).gap(1.0);
    for section in SettingsSection::ALL {
        let selected = s.settings_section == section;
        nav = nav.child(
            row()
                .key(format!("settings-{}", section.label()))
                .role("tab")
                .label(section.label())
                .h(28.0)
                .px(9.0)
                .center_y()
                .radius(k.radius.sm)
                .fill(if selected {
                    k.c.layer_02
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(k.c.hover)
                .on_press(move |s: &mut App| {
                    s.settings_section = section;
                    s.close_open();
                })
                .child(
                    text(section.label())
                        .font(if selected { k.ty.ui_medium } else { k.ty.ui })
                        .color(if selected {
                            k.c.text_base
                        } else {
                            k.c.text_muted
                        }),
                ),
        );
    }
    nav
}

fn services(s: &App, k: &Kit) -> El {
    let mut list = col().key("service-list").w(170.0).gap(1.0).child(
        text("Service")
            .font(k.ty.caption())
            .color(k.c.text_faint)
            .px(8.0)
            .pb(4.0),
    );
    for id in UploaderId::ALL {
        let selected = s.settings_service == *id;
        let enabled = s.service_enabled(*id);
        let ready = s.service_ready(*id);
        let status = if !ready {
            text("Setup").font(k.ty.caption()).color(k.c.text_faint)
        } else if !enabled {
            text("Off").font(k.ty.caption()).color(k.c.text_faint)
        } else {
            text("").font(k.ty.caption()).color(k.c.text_faint)
        };
        list = list.child(
            row()
                .key(format!("service-{}", id.name()))
                .role("tab")
                .label(id.def().display_name)
                .h(30.0)
                .px(8.0)
                .gap(8.0)
                .center_y()
                .radius(k.radius.sm)
                .fill(if selected {
                    k.c.layer_02
                } else {
                    rune::Color::TRANSPARENT
                })
                .hover(k.c.hover)
                .on_press(move |s: &mut App| s.settings_service = *id)
                .child(
                    text(id.def().display_name)
                        .font(if selected { k.ty.ui_medium } else { k.ty.ui })
                        .color(if enabled {
                            k.c.text_base
                        } else {
                            k.c.text_muted
                        })
                        .grow(1.0),
                )
                .child(status),
        );
    }

    row()
        .key("services-section")
        .w_full()
        .gap(20.0)
        .items_start()
        .child(list)
        .child(service_detail(s, k))
}

fn hairline(k: &Kit) -> El {
    el().w_full().h(1.0).fill(k.c.border_muted).a11y_hidden()
}

fn service_detail(s: &App, k: &Kit) -> El {
    let id = s.settings_service;
    let enabled = s.service_enabled(id);
    let ready = s.service_ready(id);
    let active = enabled && ready;
    let summary = format!(
        "{} · {}",
        format_size(id.max_file_size()),
        id.capabilities().names().join(", ")
    );
    let switch_help = if ready {
        "Disabled services are hidden from the upload queue."
    } else {
        "Required credentials are missing."
    };

    let mut detail = col()
        .key(format!("service-detail-{}", id.name()))
        .grow(1.0)
        .min_w(0.0)
        .gap(12.0)
        .child(
            col()
                .gap(2.0)
                .child(
                    text(id.def().display_name)
                        .font(k.ty.body_medium.lh(24.0))
                        .color(k.c.text_base),
                )
                .child(text(summary).font(k.ty.caption()).color(k.c.text_muted)),
        )
        .child(hairline(k))
        .child(
            row()
                .w_full()
                .justify_between()
                .gap(12.0)
                .center_y()
                .child(
                    col()
                        .gap(1.0)
                        .child(
                            text("Uploads enabled")
                                .font(k.ty.ui_medium)
                                .color(k.c.text_base),
                        )
                        .child(text(switch_help).font(k.ty.caption()).color(k.c.text_muted)),
                )
                .child(if ready {
                    Switch::new(active)
                        .label("Uploads enabled")
                        .hide_label()
                        .on_toggle(move |s: &mut App| {
                            let active = s.service_active(id);
                            s.set_enabled(id, !active);
                        })
                        .build(k)
                } else {
                    row().child(
                        text("Setup required")
                            .font(k.ty.caption())
                            .color(k.c.warning.fg),
                    )
                }),
        );

    let specs = fields(id);
    if specs.is_empty() {
        detail = detail.child(hairline(k)).child(
            text("This service needs no credentials.")
                .font(k.ty.ui)
                .color(k.c.text_muted),
        );
    } else {
        detail = detail.child(hairline(k));
        let mut rows = col().w_full().gap(12.0);
        for spec in specs {
            rows = rows.child(field(s, k, spec));
        }
        detail = detail.child(rows);
    }
    detail
}

fn field(s: &App, k: &Kit, spec: &'static FieldSpec) -> El {
    let revealed = s.revealed.contains(&spec.key);
    let edit = s
        .edits
        .get(&spec.key)
        .expect("every config key has an editor");
    let key = spec.key;

    let mut input = TextInput::new(edit)
        .label(spec.label)
        .placeholder(spec.placeholder)
        .width(None)
        .disabled(s.load_error.is_some())
        .on_edit(move |s: &mut App| {
            s.edits
                .get_mut(&key)
                .expect("every config key has an editor")
        })
        .on_change(move |s: &mut App, _| s.commit_field(key));

    if spec.secret {
        input = input.secure().revealed(revealed).trailing(
            IconButton::new(if revealed {
                k.glyph.eye_off
            } else {
                k.glyph.eye
            })
            .label("Show value")
            .on_press(move |s: &mut App| s.toggle_reveal(key))
            .build(k),
        );
    }

    row()
        .key(format!("field-{}", spec.label))
        .w_full()
        .gap(16.0)
        .items_start()
        .child(
            col()
                .w(170.0)
                .gap(2.0)
                .child(text(spec.label).font(k.ty.ui_medium).color(k.c.text_base))
                .child(
                    text(spec.help)
                        .font(k.ty.caption())
                        .color(k.c.text_faint)
                        .wrap(),
                ),
        )
        .child(input.build(k).grow(1.0).min_w(0.0))
}

fn preprocessing(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    let id = s.settings_service;
    let labels: Vec<&'static str> = UploaderId::ALL
        .iter()
        .map(|id| id.def().display_name)
        .collect();
    let current = id.def().display_name;
    let preprocess = s.config.get_uploader_config(id.name()).preprocess;

    let mut strategies = col().key("strategies").gap(8.0);
    for strategy in STRATEGIES.iter().copied() {
        let def = strategy.def();
        let has = s.preprocess_has(id, strategy);
        let card = col()
            .key(format!("strategy-{}", def.id))
            .padding(8.0)
            .radius(k.radius.md)
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_muted)
            .gap(3.0)
            .child(
                Checkbox::new(has)
                    .label(def.name)
                    .on_toggle(move |s: &mut App| {
                        let next = !s.preprocess_has(id, strategy);
                        s.toggle_preprocess(id, strategy, next);
                    })
                    .build(k),
            )
            .child(
                text(def.description)
                    .font(k.ty.caption())
                    .color(k.c.text_muted)
                    .wrap(),
            );
        strategies = strategies.child(card);
    }

    col()
        .key("preprocessing-section")
        .gap(12.0)
        .child(
            row()
                .gap(12.0)
                .items_end()
                .child(
                    col()
                        .gap(3.0)
                        .child(text("Service").font(k.ty.small).color(k.c.text_muted))
                        .child(
                            Select::new("preprocess-service", &labels)
                                .appearance(SelectAppearance::Field)
                                .width(220.0)
                                .label("Service")
                                .current(Some(current))
                                .open(s.is_open("preprocess-service"))
                                .on_toggle(|s: &mut App| s.toggle_open("preprocess-service"))
                                .on_close(|s: &mut App| s.close_open())
                                .on_choose(|s: &mut App, value, _| {
                                    if let Some(id) = UploaderId::ALL
                                        .iter()
                                        .find(|id| id.def().display_name == value)
                                    {
                                        s.settings_service = *id;
                                    }
                                    s.close_open();
                                })
                                .build(k, layer),
                        ),
                )
                .child(
                    Switch::new(preprocess.enabled)
                        .label("Enable preprocessing")
                        .on_toggle(move |s: &mut App| {
                            let enabled =
                                !s.config.get_uploader_config(id.name()).preprocess.enabled;
                            s.preprocess_enabled(id, enabled);
                        })
                        .build(k),
                ),
        )
        .child(
            text("Strategies use their default parameters. Edit them under Advanced.")
                .font(k.ty.caption())
                .color(k.c.text_faint),
        )
        .child(strategies)
}

fn tools(s: &App, k: &Kit) -> El {
    let mut list = col().key("tools-list").gap(8.0);
    for (tool, available) in &s.tools {
        list = list.child(
            col()
                .key(format!("tool-{}", tool.binary()))
                .gap(3.0)
                .padding(10.0)
                .radius(k.radius.md)
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_muted)
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .center_y()
                        .child(icon(&glyphs::FILE_TEXT, 16.0).color(k.c.icon_muted))
                        .child(
                            text(tool.binary())
                                .font(k.ty.ui_medium)
                                .color(k.c.text_base)
                                .grow(1.0),
                        )
                        .child(
                            Badge::new(if *available { "Installed" } else { "Not found" })
                                .variant(if *available {
                                    BadgeVariant::Neutral
                                } else {
                                    BadgeVariant::Subtle
                                })
                                .build(k),
                        ),
                )
                .child(
                    text(tool_help(*tool))
                        .font(k.ty.small)
                        .color(k.c.text_muted)
                        .wrap(),
                ),
        );
    }
    if s.tools.is_empty() {
        list = list.child(
            text("Checking for external tools.")
                .font(k.ty.ui)
                .color(k.c.text_muted),
        );
    }

    col()
        .key("tools-section")
        .gap(12.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    col()
                        .gap(2.0)
                        .child(
                            text("External tools")
                                .font(k.ty.ui_medium)
                                .color(k.c.text_base),
                        )
                        .child(
                            text("Strategies that need a missing tool are skipped at upload time.")
                                .font(k.ty.small)
                                .color(k.c.text_muted),
                        ),
                )
                .child(
                    Button::new("Refresh")
                        .variant(ButtonVariant::Outline)
                        .control_size(ControlSize::Small)
                        .on_press(|_: &mut App| {
                            let _ = worker::send(Command::ProbeTools);
                        })
                        .build(k),
                ),
        )
        .child(list)
}

fn advanced(s: &App, k: &Kit) -> El {
    let config_path = Config::path();
    let gui_path = GuiPrefs::path();

    col()
        .key("advanced-section")
        .gap(12.0)
        .child(
            col()
                .gap(8.0)
                .child(
                    text("Files")
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base),
                )
                .child(path_row(k, "Config", &config_path))
                .child(path_row(k, "GUI state", &gui_path)),
        )
        .child(
            col()
                .gap(8.0)
                .child(
                    row()
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            col()
                                .gap(2.0)
                                .child(
                                    text("config.toml")
                                        .font(k.ty.ui_medium)
                                        .color(k.c.text_base),
                                )
                                .child(
                                    text("Edit the whole file here. Applying it loads the form; saving writes it.")
                                        .font(k.ty.small)
                                        .color(k.c.text_muted),
                                ),
                        )
                        .child(
                            row()
                                .gap(6.0)
                                .child(
                                    Button::new("Load form")
                                        .variant(ButtonVariant::Ghost)
                                        .control_size(ControlSize::Mini)
                                        .on_press(|s: &mut App| s.reset_raw())
                                        .build(k),
                                )
                                .child(
                                    Button::new("Reload disk")
                                        .variant(ButtonVariant::Ghost)
                                        .control_size(ControlSize::Mini)
                                        .on_press(|s: &mut App| s.reload_config())
                                        .build(k),
                                )
                                .child(
                                    Button::new("Apply TOML")
                                        .variant(ButtonVariant::Outline)
                                        .control_size(ControlSize::Mini)
                                        .on_press(|s: &mut App| s.apply_raw())
                                        .build(k),
                                ),
                        ),
                )
                .child(
                    TextArea::new(&s.raw)
                        .label("config.toml")
                        .rows(16)
                        .width(None)
                        .on_edit(|s: &mut App| &mut s.raw)
                        .build(k),
                )
                .child(
                    text("Applying TOML replaces the form values and marks them unsaved. Nothing is written until you press Save changes.")
                        .font(k.ty.small)
                        .color(k.c.text_faint),
                ),
        )
}

fn path_row(k: &Kit, label: &str, path: &Path) -> El {
    let value = path.display().to_string();
    row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(
            text(format!("{label}:"))
                .font(k.ty.small)
                .color(k.c.text_faint)
                .w(70.0),
        )
        .child(
            text(value.clone())
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .selectable(true)
                .grow(1.0)
                .shrink()
                .truncate(),
        )
        .child(copy_button(k, value.clone(), "Copy path"))
        .child(
            Button::new("Open")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Mini)
                .on_press(move |_: &mut App| reveal_path(Path::new(&value)))
                .build(k),
        )
}

fn tool_help(tool: Tool) -> &'static str {
    match tool {
        Tool::Ffmpeg => "Splits videos into playable segments for the split_video strategy.",
        Tool::Ffprobe => "Reads video metadata so segments are sized correctly.",
    }
}

fn format_size(size: FileSize) -> String {
    if size.is_unbounded() {
        "No limit".to_string()
    } else {
        size.to_human_string()
    }
}

fn reveal_path(path: &Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    let _ = result;
}
