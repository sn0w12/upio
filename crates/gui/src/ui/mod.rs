mod settings;
mod upload;

mod icons {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

use rune::{col, row, text, Cx, El};
use rune_kit::{Kit, SegmentedControl};

use crate::app::{App, Page};
use crate::theme::ThemeMode;

const MOTION_SCALE: f32 = 1.15;

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    cx.time_scale(MOTION_SCALE);
    let k = s.kit(cx.system_dark());
    crate::window_theme::apply(k.c.bg_deep, k.c.icon_base, k.dark);
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
                .child(content),
        );
    rune_kit::root(body, layer)
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
