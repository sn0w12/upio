use serde::{Deserialize, Serialize};

use rune::Color;
use rune_kit::{Kit, Palette, StateColors};

use crate::coss::{mix, over, Coss};

/// The user's theme preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "Auto",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn dark(self, system_dark: bool) -> bool {
        match self {
            Self::System => system_dark,
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

pub fn kit(dark: bool) -> Kit {
    let mut kit = Kit::neutral(dark);
    kit.c = palette(&Coss::new(dark));
    kit.radius.sm = 6.0;
    kit.radius.md = 8.0;
    kit.radius.lg = 11.0;
    kit.radius.xl = 14.0;
    kit
}

fn palette(c: &Coss) -> Palette {
    let mut p = Kit::neutral(c.dark).c;
    let base = c.bg;
    let card = c.card;
    let secondary = over(c.secondary, card);
    let accent = over(c.accent, card);
    let faint = mix(c.muted_fg, base, 0.62);
    let sheen = |a: f32| {
        if c.dark {
            Color::BLACK.alpha(a)
        } else {
            Color::WHITE.alpha(a)
        }
    };
    let state = |soft: Color, fg: Color, strong: Color| StateColors {
        bg: over(soft, card),
        fg,
        border: over(strong.alpha(0.28), card),
        icon: fg,
    };

    p.page = base;
    p.bg_base = base;
    p.bg_deep = c.sidebar;
    p.layer_01 = card;
    p.layer_02 = secondary;
    p.layer_03 = over(sheen(0.05), card);
    p.layer_04 = over(sheen(0.09), card);
    p.bg_inverse = c.primary;
    p.bg_contrast = c.primary;
    p.bg_button_neutral = secondary;
    p.bg_icon_button_contrast = c.primary;
    p.bg_accent = c.primary;
    p.submit_highlight = sheen(0.12);
    p.submit_hover = sheen(0.16);
    p.submit_fg = c.primary_fg;
    p.submit_icon = c.primary_fg;

    p.text_base = c.fg;
    p.text_muted = c.muted_fg;
    p.text_faint = faint;
    p.text_inverse = c.primary_fg;
    p.text_contrast = c.primary_fg;
    p.text_accent = c.fg;

    p.icon_base = c.fg;
    p.icon_muted = c.muted_fg;
    p.icon_faint = faint;
    p.icon_inverse = c.primary_fg;
    p.icon_contrast = c.primary_fg;
    p.icon_accent = c.fg;

    p.border_muted = c.sidebar_border;
    p.border_base = c.border;
    p.border_strong = c.input;
    p.border_focus = c.ring;

    p.hover = accent;
    p.pressed = over(sheen(0.06), card);
    p.contrast_hover = sheen(0.10);
    p.contrast_pressed = sheen(0.16);
    p.scrim = Color::BLACK.alpha(0.45);
    p.depth_top = Color::WHITE.alpha(0.18);
    p.depth_bottom = Color::BLACK.alpha(0.10);
    p.white = Color::WHITE;
    p.black = Color::BLACK;
    p.knob = over(Color::WHITE, card);
    p.knob_on = c.primary_fg;

    p.success = state(c.success_soft, c.success_fg, c.success);
    p.warning = state(c.warning_soft, c.warning_fg, c.warning);
    p.danger = state(c.destructive_soft, c.destructive_fg, c.destructive);
    p.info = state(c.info_soft, c.info_fg, c.info);

    p.surface_base = card;
    p.surface_weak = secondary;
    p.surface_hover = accent;
    p.surface_raised = card;
    p.surface_raised_base = card;
    p.surface_raised_hover = secondary;
    p.surface_float = c.popover;
    p.surface_disabled = secondary;
    p.border_weak = c.sidebar_border;
    p.border_hover = c.input;
    p.border_active = c.input;
    p.border_disabled = c.sidebar_border;
    p.text_strong = c.fg;
    p.text_weak = faint;
    p.text_invert_strong = c.primary_fg;
    p.icon_weaker = faint;
    p.std_border = c.border;
    p.std_icon = c.fg;
    p.std_icon_invert = c.primary_fg;
    p.std_icon_strong = c.fg;
    p.std_text = c.fg;

    p
}
