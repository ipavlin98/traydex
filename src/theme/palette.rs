use super::Theme;
use gpui::{Hsla, Rgba};
use serde::{Deserialize, Serialize};

pub const ACCENTS: [Hex; 8] = [
    Hex(0x3b82f6),
    Hex(0x8b5cf6),
    Hex(0xec4899),
    Hex(0xef4444),
    Hex(0xf97316),
    Hex(0x22c55e),
    Hex(0x14b8a6),
    Hex(0xf59e0b),
];
pub const BACKGROUNDS: [Hex; 8] = [
    Hex(0xffffff),
    Hex(0xf6f3ec),
    Hex(0xeef2f7),
    Hex(0x1f1f23),
    Hex(0x000000),
    Hex(0x0f172a),
    Hex(0x1e1e2e),
    Hex(0x002b36),
];
const OFFERED: usize = 7;

const SURFACE: Share = Share { light: 0.022, dark: 0.042 };
const HOVER: Share = Share { light: 0.056, dark: 0.11 };
const ACTIVE: Share = Share { light: 0.1, dark: 0.152 };
const BORDER: Share = Share { light: 0.086, dark: 0.131 };
const BORDER_STRONG: Share = Share { light: 0.185, dark: 0.215 };
const MUTED: Share = Share { light: 0.69, dark: 0.68 };
const FAINT: Share = Share { light: 0.435, dark: 0.45 };
const TINT: Share = Share { light: 0.08, dark: 0.12 };
const SELECTION: Share = Share { light: 0.2, dark: 0.4 };

const DARK_TEXT: f32 = 0.91;
const LIGHT_TEXT: f32 = 0.93;
const READABLE: f32 = 3.;
const LEGIBLE: f32 = 4.5;
const LEGIBLE_STEPS: u8 = 10;
const LUMINANCE: [f32; 3] = [0.2126, 0.7152, 0.0722];
const LINEAR_KNEE: f32 = 0.04045;
const LINEAR_SLOPE: f32 = 12.92;
const GAMMA_OFFSET: f32 = 0.055;
const GAMMA_SCALE: f32 = 1.055;
const GAMMA: f32 = 2.4;
const CONTRAST_OFFSET: f32 = 0.05;
const SECTORS: f32 = 6.;
const EPSILON: f32 = 1e-6;

#[derive(Clone, Copy)]
struct Share {
    light: f32,
    dark: f32,
}

impl Share {
    fn at(self, background: Rgba) -> f32 {
        let lightness = Hsla::from(background).l;
        self.dark + (self.light - self.dark) * lightness
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hex(u32);

impl Hex {
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.trim().strip_prefix('#')?;
        if digits.len() != 6 {
            return None;
        }
        u32::from_str_radix(digits, 16).ok().map(Self)
    }

    pub fn of(color: Rgba) -> Self {
        let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
        Self(channel(color.r) << 16 | channel(color.g) << 8 | channel(color.b))
    }

    pub fn color(self) -> Rgba {
        gpui::rgb(self.0)
    }
}

impl TryFrom<String> for Hex {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| format!("`{text}` is not a #rrggbb color"))
    }
}

impl From<Hex> for String {
    fn from(hex: Hex) -> Self {
        format!("#{:06x}", hex.0)
    }
}

pub fn parse_color(text: &str) -> Option<Rgba> {
    let digits = text.trim().strip_prefix('#')?;
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        6 => Some(gpui::rgb(value)),
        8 => Some(gpui::rgba(value)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
}

impl Hsv {
    pub fn of(color: Rgba) -> Self {
        let Rgba { r, g, b, .. } = color;
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let hue = if delta <= EPSILON {
            0.
        } else if max == r {
            ((g - b) / delta).rem_euclid(SECTORS) / SECTORS
        } else if max == g {
            ((b - r) / delta + 2.) / SECTORS
        } else {
            ((r - g) / delta + 4.) / SECTORS
        };
        let saturation = if max <= EPSILON { 0. } else { delta / max };
        Self { hue, saturation, value: max }
    }

    pub fn color(self) -> Rgba {
        let sector = self.hue.rem_euclid(1.) * SECTORS;
        let chroma = self.value * self.saturation;
        let rising = chroma * (1. - (sector.rem_euclid(2.) - 1.).abs());
        let (r, g, b) = match sector as u8 {
            0 => (chroma, rising, 0.),
            1 => (rising, chroma, 0.),
            2 => (0., chroma, rising),
            3 => (0., rising, chroma),
            4 => (rising, 0., chroma),
            _ => (chroma, 0., rising),
        };
        let lift = self.value - chroma;
        Rgba { r: r + lift, g: g + lift, b: b + lift, a: 1. }
    }

    pub fn pure(hue: f32) -> Rgba {
        Self { hue, saturation: 1., value: 1. }.color()
    }
}

pub fn offered(presets: &'static [Hex], own: Rgba) -> impl Iterator<Item = (usize, Hex)> {
    let own = Hex::of(own);
    presets.iter().copied().enumerate().filter(move |(_, color)| *color != own).take(OFFERED)
}

pub fn text_for(background: Rgba) -> Rgba {
    let middle = mix(black(), white(), 0.5);
    if luminance(background) > luminance(middle) {
        mix(background, black(), DARK_TEXT)
    } else {
        mix(background, white(), LIGHT_TEXT)
    }
}

pub fn readable(accent: Rgba, background: Rgba) -> bool {
    contrast(accent, background) >= READABLE
}

pub fn contrast(first: Rgba, second: Rgba) -> f32 {
    let (first, second) = (luminance(first), luminance(second));
    (first.max(second) + CONTRAST_OFFSET) / (first.min(second) + CONTRAST_OFFSET)
}

pub fn dark(background: Rgba) -> bool {
    luminance(background) <= luminance(mix(black(), white(), 0.5))
}

pub(super) fn neutrals(background: Rgba, text: Rgba) -> Theme {
    let base = if dark(background) { Theme::dark() } else { Theme::light() };
    let shade = |share: Share| mix(background, text, share.at(background));
    Theme {
        bg: background,
        surface: shade(SURFACE),
        hover: shade(HOVER),
        active: shade(ACTIVE),
        border: shade(BORDER),
        border_strong: shade(BORDER_STRONG),
        text,
        muted: shade(MUTED),
        faint: shade(FAINT),
        ..base
    }
}

pub(super) fn accented(theme: Theme, accent: Rgba) -> Theme {
    let accent = Rgba { a: 1., ..accent };
    let legible = (0..=LEGIBLE_STEPS)
        .map(|step| mix(accent, theme.text, f32::from(step) / f32::from(LEGIBLE_STEPS)))
        .find(|color| contrast(*color, theme.bg) >= LEGIBLE)
        .unwrap_or(theme.text);
    Theme {
        accent,
        accent_text: legible,
        accent_tint: Rgba { a: TINT.at(theme.bg), ..accent },
        selection: Rgba { a: SELECTION.at(theme.bg), ..accent },
        on_accent: text_for(accent),
        brand: accent,
        ..theme
    }
}

fn luminance(color: Rgba) -> f32 {
    let linear = |channel: f32| {
        if channel <= LINEAR_KNEE { channel / LINEAR_SLOPE } else { ((channel + GAMMA_OFFSET) / GAMMA_SCALE).powf(GAMMA) }
    };
    let [r, g, b] = LUMINANCE;
    r * linear(color.r) + g * linear(color.g) + b * linear(color.b)
}

fn mix(from: Rgba, to: Rgba, share: f32) -> Rgba {
    let blend = |from: f32, to: f32| from + (to - from) * share;
    Rgba { r: blend(from.r, to.r), g: blend(from.g, to.g), b: blend(from.b, to.b), a: 1. }
}

fn black() -> Rgba {
    Rgba { r: 0., g: 0., b: 0., a: 1. }
}

fn white() -> Rgba {
    Rgba { r: 1., g: 1., b: 1., a: 1. }
}
