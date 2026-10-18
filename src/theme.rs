use gpui::{Background, Global, Hsla, Rgba, WindowAppearance, linear_color_stop, linear_gradient, rgb, rgba};
use serde::{Deserialize, Serialize};

pub fn fade(color: Rgba, angle: f32) -> Background {
    fade_until(color, angle, 1.)
}

pub fn fade_until(color: Rgba, angle: f32, solid: f32) -> Background {
    linear_gradient(angle, linear_color_stop(Hsla::from(color).opacity(0.), 0.), linear_color_stop(color, solid))
}

#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Rgba,
    pub surface: Rgba,
    pub hover: Rgba,
    pub active: Rgba,
    pub border: Rgba,
    pub border_strong: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    pub accent: Rgba,
    pub accent_text: Rgba,
    pub accent_tint: Rgba,
    pub selection: Rgba,
    pub danger: Rgba,
    pub danger_tint: Rgba,
    pub success: Rgba,
    pub knob: Rgba,
    pub on_accent: Rgba,
    pub brand: Rgba,
    pub code_ink: Rgba,
    pub code_paper: Rgba,
}

impl Global for Theme {}

impl Theme {
    pub fn resolve(mode: ThemeMode, appearance: WindowAppearance) -> Self {
        let dark = match mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark),
        };
        if dark { Self::dark() } else { Self::light() }
    }

    fn dark() -> Self {
        Self {
            bg: rgb(0x000000),
            surface: rgb(0x0a0a0a),
            hover: rgb(0x1a1a1a),
            active: rgb(0x242424),
            border: rgb(0x1f1f1f),
            border_strong: rgb(0x333333),
            text: rgb(0xededed),
            muted: rgb(0xa1a1a1),
            faint: rgb(0x6b6b6b),
            accent: rgb(0xb4a3ff),
            accent_text: rgb(0xc5b8ff),
            accent_tint: rgba(0xb4a3ff1f),
            selection: rgba(0xb4a3ff66),
            danger: rgb(0xff6166),
            danger_tint: rgba(0xff61661f),
            success: rgb(0x4ade80),
            knob: rgb(0xffffff),
            on_accent: rgb(0x17161f),
            brand: rgb(0xb4a3ff),
            code_ink: rgb(0x000000),
            code_paper: rgb(0xffffff),
        }
    }

    fn light() -> Self {
        Self {
            bg: rgb(0xffffff),
            surface: rgb(0xfafafa),
            hover: rgb(0xf2f2f2),
            active: rgb(0xe8e8e8),
            border: rgb(0xebebeb),
            border_strong: rgb(0xd4d4d4),
            text: rgb(0x171717),
            muted: rgb(0x5f5f5f),
            faint: rgb(0x9a9a9a),
            accent: rgb(0x7b61ff),
            accent_text: rgb(0x6247e8),
            accent_tint: rgba(0x7b61ff14),
            selection: rgba(0x7b61ff33),
            danger: rgb(0xd4252b),
            danger_tint: rgba(0xd4252b0c),
            success: rgb(0x16a34a),
            knob: rgb(0xffffff),
            on_accent: rgb(0xffffff),
            brand: rgb(0x7b61ff),
            code_ink: rgb(0x000000),
            code_paper: rgb(0xffffff),
        }
    }
}

#[cfg(test)]
mod tests;
