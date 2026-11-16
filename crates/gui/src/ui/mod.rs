mod settings;
mod upload;

use rune::{col, row, text, Cx, El, Tx};
use rune_kit::{ControlSize, IconButton, Kit, SegmentedControl};

use crate::app::{App, Page};
use crate::theme::ThemeMode;

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let k = s.kit(cx.system_dark());
    let insets = cx.chrome_insets();
    let bar_height = if insets.height > 0.0 {
        insets.height
    } else {
        32.0
    };
    let mut layer = Vec::new();

    let content = match s.page {
        Page::Upload => upload::view(s, &k, &mut layer),
        Page::Settings => settings::view(s, &k, &mut layer),
    };

    let body = col()
        .key("window")
        .w_full()
        .h_full()
        .fill(k.c.bg_base)
        .child(
            col()
                .key("content")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .pt(bar_height)
                .child(content),
        )
        .child(
            titlebar(&k, insets)
                .w_full()
                .h(bar_height)
                .absolute(0.0, 0.0),
        );
    rune_kit::root(body, layer)
}

fn titlebar(k: &Kit, insets: rune::ChromeInsets) -> El {
    row()
        .key("titlebar")
        .h_full()
        .w_full()
        .pl(insets.leading)
        .pr(insets.trailing)
        .fill(k.c.bg_deep)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_muted)
        .window_drag()
}

pub fn notice(s: &App, k: &Kit) -> Option<El> {
    let message = s.notice.as_ref()?;
    Some(
        row()
            .key("notice")
            .w_full()
            .px(10.0)
            .py(7.0)
            .radius(k.radius.md)
            .fill(k.c.info.bg)
            .border(1.0, k.c.info.border)
            .child(
                text(message)
                    .key("message")
                    .font(k.ty.ui)
                    .color(k.c.info.fg)
                    .grow(1.0)
                    .wrap(),
            ),
    )
}

pub fn page_header(k: &Kit, title: &str, subtitle: &str) -> El {
    col()
        .key("page-header")
        .gap(2.0)
        .child(
            text(title)
                .key("title")
                .font(k.ty.body_medium.lh(26.0))
                .color(k.c.text_base),
        )
        .child(
            text(subtitle)
                .key("subtitle")
                .font(k.ty.caption())
                .color(k.c.text_muted)
                .wrap(),
        )
}

pub fn theme_picker(s: &App, k: &Kit) -> El {
    SegmentedControl::new(
        &[
            ThemeMode::System.label(),
            ThemeMode::Light.label(),
            ThemeMode::Dark.label(),
        ],
        Some(match s.theme {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        }),
    )
    .label("Theme")
    .width(Some(220.0))
    .on_pick(|s: &mut App, index, _| {
        s.set_theme(match index {
            1 => ThemeMode::Light,
            2 => ThemeMode::Dark,
            _ => ThemeMode::System,
        });
    })
    .build(k)
}

pub fn copy_button(k: &Kit, value: String, label: &str) -> El {
    IconButton::new(k.glyph.copy)
        .label(label)
        .control_size(ControlSize::Mini)
        .on_press_tx(move |_: &mut App, tx: &mut Tx| tx.copy(value.clone()))
        .build(k)
}
