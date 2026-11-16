//! coss/ui colour tokens, copied from `rune-coss-ui` so both apps render the same palette.

use rune::color::{oklch, oklcha, srgb_decode, srgb_encode};
use rune::Color;

const N50: f32 = 0.985;
const N100: f32 = 0.970;
const N400: f32 = 0.708;
const N500: f32 = 0.556;
const N800: f32 = 0.269;
const N950: f32 = 0.145;

fn neutral(l: f32) -> Color {
    oklch(l, 0.0, 0.0)
}

fn white() -> Color {
    oklch(1.0, 0.0, 0.0)
}

fn black_at(a: f32) -> Color {
    oklcha(0.0, 0.0, 0.0, a)
}

fn white_at(a: f32) -> Color {
    oklcha(1.0, 0.0, 0.0, a)
}

fn red500() -> Color {
    oklch(0.637, 0.237, 25.331)
}

fn red400() -> Color {
    oklch(0.704, 0.191, 22.216)
}

fn red700() -> Color {
    oklch(0.505, 0.213, 27.518)
}

fn blue500() -> Color {
    oklch(0.623, 0.214, 259.815)
}

fn blue700() -> Color {
    oklch(0.488, 0.243, 264.376)
}

fn blue400() -> Color {
    oklch(0.707, 0.165, 254.624)
}

fn emerald500() -> Color {
    oklch(0.696, 0.17, 162.48)
}

fn emerald700() -> Color {
    oklch(0.508, 0.118, 165.612)
}

fn emerald400() -> Color {
    oklch(0.765, 0.177, 163.223)
}

fn amber500() -> Color {
    oklch(0.769, 0.188, 70.08)
}

fn amber700() -> Color {
    oklch(0.555, 0.163, 48.998)
}

fn amber400() -> Color {
    oklch(0.828, 0.189, 84.429)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let channel = |x: f32, y: f32| srgb_decode(srgb_encode(x) * t + srgb_encode(y) * (1.0 - t));
    Color {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: a.a * t + b.a * (1.0 - t),
    }
}

pub fn over(top: Color, bottom: Color) -> Color {
    let a = top.a + bottom.a * (1.0 - top.a);
    if a <= 0.0 {
        return Color::TRANSPARENT;
    }
    let channel = |t: f32, b: f32| {
        let (t, b) = (srgb_encode(t), srgb_encode(b));
        srgb_decode((t * top.a + b * bottom.a * (1.0 - top.a)) / a)
    };
    Color {
        r: channel(top.r, bottom.r),
        g: channel(top.g, bottom.g),
        b: channel(top.b, bottom.b),
        a,
    }
}

pub struct Coss {
    pub dark: bool,
    pub bg: Color,
    pub card: Color,
    pub sidebar: Color,
    pub sidebar_border: Color,
    pub fg: Color,
    pub primary: Color,
    pub primary_fg: Color,
    pub secondary: Color,
    pub accent: Color,
    pub border: Color,
    pub input: Color,
    pub ring: Color,
    pub popover: Color,
    pub destructive: Color,
    pub destructive_fg: Color,
    pub destructive_soft: Color,
    pub muted_fg: Color,
    pub info: Color,
    pub info_fg: Color,
    pub info_soft: Color,
    pub success: Color,
    pub success_fg: Color,
    pub success_soft: Color,
    pub warning: Color,
    pub warning_fg: Color,
    pub warning_soft: Color,
}

impl Coss {
    pub fn new(dark: bool) -> Coss {
        if dark {
            let bg = mix(neutral(N950), white(), 0.95);
            let input = white_at(0.08);
            let destructive = mix(red500(), white(), 0.9);
            let info = blue500();
            let success = emerald500();
            let warning = amber500();
            Coss {
                dark,
                bg,
                card: mix(bg, white(), 0.98),
                sidebar: mix(neutral(N950), white(), 0.97),
                sidebar_border: white_at(0.05),
                fg: neutral(N100),
                primary: neutral(N100),
                primary_fg: neutral(N800),
                secondary: white_at(0.04),
                accent: white_at(0.04),
                border: white_at(0.06),
                input,
                ring: neutral(N500),
                popover: mix(bg, white(), 0.98),
                destructive,
                destructive_fg: red400(),
                destructive_soft: destructive.alpha(0.16),
                muted_fg: mix(neutral(N500), white(), 0.9),
                info,
                info_fg: blue400(),
                info_soft: info.alpha(0.16),
                success,
                success_fg: emerald400(),
                success_soft: success.alpha(0.16),
                warning,
                warning_fg: amber400(),
                warning_soft: warning.alpha(0.16),
            }
        } else {
            let accent = black_at(0.04);
            let destructive = red500();
            let info = blue500();
            let success = emerald500();
            let warning = amber500();
            Coss {
                dark,
                bg: white(),
                card: white(),
                sidebar: neutral(0.985),
                sidebar_border: black_at(0.06),
                fg: neutral(N800),
                primary: neutral(N800),
                primary_fg: neutral(N50),
                secondary: black_at(0.04),
                accent,
                border: black_at(0.08),
                input: black_at(0.10),
                ring: neutral(N400),
                popover: white(),
                destructive,
                destructive_fg: red700(),
                destructive_soft: destructive.alpha(0.08),
                muted_fg: mix(neutral(N500), black_at(1.0), 0.9),
                info,
                info_fg: blue700(),
                info_soft: info.alpha(0.08),
                success,
                success_fg: emerald700(),
                success_soft: success.alpha(0.08),
                warning,
                warning_fg: amber700(),
                warning_soft: warning.alpha(0.08),
            }
        }
    }
}
