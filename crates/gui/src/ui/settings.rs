use std::path::Path;

use rune::{col, el, row, text, El, ScrollSpec};
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit,
    ScrollBar, ScrollBarStyle, Switch, TextArea, TextInput,
};
use upio::filesize::FileSize;
use upio::preprocess::tools::Tool;
use upio::preprocess::STRATEGIES;
use upio::registry::UploaderId;
use upio_config::{Config, ConfigKey};

use crate::app::{App, GuiPrefs, Page, SettingsSection};

use super::{notice, theme_picker};

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

pub fn view(s: &App, k: &Kit) -> El {
    let mut page = col()
        .key("settings-page")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .min_h(0.0)
        .padding(12.0)
        .pr(6.0)
        .gap(0.0);

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
        SettingsSection::Tools => tools(s, k),
        SettingsSection::Advanced => advanced(s, k),
    };
    let content = content.pr(10.0);

    page.child(
        row()
            .key("settings-body")
            .grow(1.0)
            .min_h(0.0)
            .gap(12.0)
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
                    .pl(12.0)
                    .pr(6.0)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .child(content)
                    .scroll_bar_styled(
                        ScrollBarStyle::native(k.c.border_muted, k.c.border_strong).inside(),
                    ),
            ),
    )
    .child(settings_header(s, k))
}

fn settings_header(s: &App, k: &Kit) -> El {
    row()
        .key("settings-header")
        .pin(None, Some(12.0), Some(12.0), None)
        .gap(6.0)
        .center_y()
        .child(
            IconButton::new(&crate::ui::icons::X)
                .label("Back")
                .variant(IconButtonVariant::Ghost)
                .control_size(ControlSize::Large)
                .on_press(|s: &mut App| s.set_page(Page::Upload))
                .build(k),
        )
        .child(
            IconButton::new(&crate::ui::icons::TRASH)
                .label("Discard")
                .variant(IconButtonVariant::Ghost)
                .control_size(ControlSize::Large)
                .disabled(!s.dirty)
                .on_press(|s: &mut App| s.discard_changes())
                .build(k),
        )
        .child(
            IconButton::new(&crate::ui::icons::SAVE)
                .label("Save Changes")
                .variant(IconButtonVariant::Contrast)
                .control_size(ControlSize::Large)
                .disabled(!s.dirty || s.load_error.is_some())
                .on_press(|s: &mut App| s.save_config())
                .build(k),
        )
}

fn appearance(s: &App, k: &Kit) -> El {
    col()
        .key("appearance-section")
        .w_full()
        .gap(20.0)
        .child(section(
            k,
            "Appearance",
            "How upio looks on this machine.",
            setting_row(
                k,
                "Theme",
                "Follow the system appearance, or pin light or dark.",
                theme_picker(s, k),
            ),
        ))
}

fn heading(k: &Kit, title: &str, description: &str) -> El {
    col()
        .w_full()
        .gap(2.0)
        .child(text(title).font(k.ty.ui_medium).color(k.c.text_base))
        .child(
            text(description)
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        )
}

fn section(k: &Kit, title: &str, description: &str, body: El) -> El {
    col()
        .w_full()
        .gap(8.0)
        .child(heading(k, title, description))
        .child(hairline(k))
        .child(body)
}

fn rows(k: &Kit, items: Vec<El>) -> El {
    let mut out = col().w_full().gap(0.0);
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            out = out.child(hairline(k));
        }
        out = out.child(item);
    }
    out
}

fn setting_label(k: &Kit, title: &str, help: &str) -> El {
    let mut label = col()
        .gap(2.0)
        .child(text(title).font(k.ty.ui_medium).color(k.c.text_base).wrap());
    if !help.is_empty() {
        label = label.child(text(help).font(k.ty.caption()).color(k.c.text_faint).wrap());
    }
    label
}

fn setting_row(k: &Kit, title: &str, help: &str, control: El) -> El {
    row()
        .w_full()
        .gap(16.0)
        .items_start()
        .py(10.0)
        .child(setting_label(k, title, help).grow(1.0).min_w(0.0))
        .child(control)
}

fn tab_row(k: &Kit, key: String, label: &'static str, selected: bool) -> El {
    row()
        .key(key)
        .role("tab")
        .label(label)
        .h(28.0)
        .px(9.0)
        .gap(8.0)
        .center_y()
        .radius(k.radius.sm)
        .fill(if selected {
            k.c.layer_02
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(k.c.hover)
}

fn settings_nav(s: &App, k: &Kit) -> El {
    let mut nav = col().key("settings-nav").w(150.0).gap(1.0);
    for section in SettingsSection::ALL {
        let selected = s.settings_section == section;
        nav = nav.child(
            tab_row(
                k,
                format!("settings-{}", section.label()),
                section.label(),
                selected,
            )
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
    let mut list = col().key("service-list").w(170.0).gap(1.0).sticky_top(0.0);
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
            tab_row(
                k,
                format!("service-{}", id.name()),
                id.def().display_name,
                selected,
            )
            .on_press(move |s: &mut App| s.settings_service = *id)
            .child(
                text(id.def().display_name)
                    .font(if selected { k.ty.ui_medium } else { k.ty.ui })
                    .color(if selected {
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
        .gap(12.0)
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

    let enabled_control = if ready {
        Switch::new(active)
            .label("Uploads enabled")
            .hide_label()
            .on_toggle(move |s: &mut App| {
                let active = s.service_active(id);
                s.set_enabled(id, !active);
            })
            .build(k)
    } else {
        text("Setup required")
            .font(k.ty.caption())
            .color(k.c.warning.fg)
    };

    let specs = fields(id);
    let authentication = if specs.is_empty() {
        text("This service needs no credentials.")
            .font(k.ty.ui)
            .color(k.c.text_muted)
    } else {
        let mut items = Vec::new();
        for spec in specs {
            items.push(field(s, k, spec));
        }
        rows(k, items)
    };

    col()
        .key(format!("service-detail-{}", id.name()))
        .grow(1.0)
        .min_w(0.0)
        .gap(16.0)
        .child(heading(k, id.def().display_name, &summary))
        .child(section(
            k,
            "Uploads",
            "Choose whether this service takes new files.",
            setting_row(
                k,
                "Uploads enabled",
                if ready {
                    "Disabled services are hidden from the upload queue."
                } else {
                    "Add the missing credentials below to turn this service on."
                },
                enabled_control,
            ),
        ))
        .child(section(
            k,
            "Authentication",
            "Credentials for this service. They are stored in config.toml.",
            authentication,
        ))
        .child(preprocessing(s, k, id))
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
        .w_full()
        .gap(16.0)
        .items_start()
        .py(10.0)
        .child(setting_label(k, spec.label, spec.help).grow(1.0).min_w(0.0))
        .child(input.build(k).w(280.0).key(format!("input-{}", spec.label)))
        .key(format!("field-{}", spec.label))
}

fn preprocessing(s: &App, k: &Kit, id: UploaderId) -> El {
    let preprocess = s.config.get_uploader_config(id.name()).preprocess;

    let mut items = vec![setting_row(
        k,
        "Enabled",
        "Run the selected strategies on every file sent to this service.",
        Switch::new(preprocess.enabled)
            .label("Enable preprocessing")
            .hide_label()
            .on_toggle(move |s: &mut App| {
                let enabled = !s.config.get_uploader_config(id.name()).preprocess.enabled;
                s.preprocess_enabled(id, enabled);
            })
            .build(k),
    )];
    for strategy in STRATEGIES.iter().copied() {
        let def = strategy.def();
        let has = s.preprocess_has(id, strategy);
        items.push(
            setting_row(
                k,
                def.name,
                def.description,
                Switch::new(has)
                    .label(def.name)
                    .hide_label()
                    .on_toggle(move |s: &mut App| {
                        s.toggle_preprocess_strategy(id, strategy);
                    })
                    .build(k),
            )
            .key(format!("strategy-{}", def.id)),
        );
    }

    section(
        k,
        "Preprocessing",
        "Prepare files before upload. Strategies use their default parameters; edit them in Advanced.",
        rows(k, items),
    )
}

fn tools(s: &App, k: &Kit) -> El {
    let body = if s.tools.is_empty() {
        text("Checking for external tools.")
            .font(k.ty.ui)
            .color(k.c.text_muted)
    } else {
        let mut items = Vec::new();
        for (tool, available) in s.tools.iter() {
            items.push(setting_row(
                k,
                tool.binary(),
                tool_help(*tool),
                Badge::new(if *available { "Installed" } else { "Not found" })
                    .variant(if *available {
                        BadgeVariant::Neutral
                    } else {
                        BadgeVariant::Subtle
                    })
                    .build(k),
            ));
        }
        rows(k, items)
    };

    col().key("tools-section").w_full().gap(20.0).child(section(
        k,
        "External tools",
        "Strategies that need a missing tool are skipped at upload time.",
        body,
    ))
}

fn advanced(s: &App, k: &Kit) -> El {
    let config_path = Config::path();
    let gui_path = GuiPrefs::path();

    let files = rows(
        k,
        vec![
            path_row(k, "Config", &config_path),
            path_row(k, "GUI state", &gui_path),
        ],
    );

    let mut raw = col().w_full().gap(12.0).child(setting_row(
        k,
        "Raw editor",
        "The whole config.toml, including service tokens, in plain text.",
        Button::new(if s.show_raw { "Hide" } else { "Show" })
            .variant(ButtonVariant::Outline)
            .on_press(|s: &mut App| s.show_raw = !s.show_raw)
            .build(k)
            .key("raw-toggle"),
    ));
    if s.show_raw {
        raw = raw
            .child(hairline(k))
            .child(
                TextArea::new(&s.raw)
                    .label("config.toml")
                    .rows(16)
                    .width(None)
                    .on_edit(|s: &mut App| &mut s.raw)
                    .build(k),
            )
            .child(
                row()
                    .w_full()
                    .justify_between()
                    .gap(12.0)
                    .center_y()
                    .child(
                        text("Applying TOML replaces the form values and marks them unsaved. Nothing is written until you save.")
                            .font(k.ty.small)
                            .color(k.c.text_faint)
                            .grow(1.0)
                            .wrap(),
                    )
                    .child(
                        row()
                            .gap(6.0)
                            .child(
                                Button::new("Load form")
                                    .variant(ButtonVariant::Ghost)
                                    .on_press(|s: &mut App| s.reset_raw())
                                    .build(k),
                            )
                            .child(
                                Button::new("Reload disk")
                                    .variant(ButtonVariant::Ghost)
                                    .on_press(|s: &mut App| s.reload_config())
                                    .build(k),
                            )
                            .child(
                                Button::new("Apply TOML")
                                    .variant(ButtonVariant::Outline)
                                    .on_press(|s: &mut App| s.apply_raw())
                                    .build(k),
                            ),
                    ),
            );
    }

    col()
        .key("advanced-section")
        .w_full()
        .gap(20.0)
        .child(section(
            k,
            "Files",
            "Where upio keeps its configuration.",
            files,
        ))
        .child(section(
            k,
            "Configuration file",
            "View or edit the raw config.toml. Tokens stay hidden until you open it.",
            raw,
        ))
}

fn path_row(k: &Kit, label: &str, path: &Path) -> El {
    let value = path.display().to_string();
    row()
        .w_full()
        .gap(16.0)
        .items_start()
        .py(10.0)
        .child(
            text(label)
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .w(170.0),
        )
        .child(
            text(value)
                .font(k.ty.mono)
                .color(k.c.text_muted)
                .selectable(true)
                .grow(1.0)
                .shrink()
                .truncate(),
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
