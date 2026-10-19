use super::Layout;
use crate::i18n::Language;
use crate::keys::Shortcuts;
use crate::theme::ThemeMode;
use gpui::WindowBackgroundAppearance;
use serde::{Deserialize, Deserializer, Serialize};

pub const DEFAULT_SIZE: (f32, f32) = (540., 540.);
pub const MIN_SIZE: (f32, f32) = DEFAULT_SIZE;
const MAX_TRANSPARENCY: u8 = 40;
pub const FONT_SIZES: [u8; 5] = [12, 13, 14, 15, 16];
const DEFAULT_FONT_SIZE: u8 = 13;
const MAX_SIZE: f32 = 2000.;

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight];
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShakeLevel {
    #[default]
    Off,
    Low,
    High,
}

impl ShakeLevel {
    pub const ALL: [ShakeLevel; 3] = [ShakeLevel::Off, ShakeLevel::Low, ShakeLevel::High];
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub enum TileInput {
    #[default]
    Clipboard,
    Selection,
    Picker,
}

impl TileInput {
    pub const ALL: [TileInput; 3] = [TileInput::Clipboard, TileInput::Selection, TileInput::Picker];
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub automatic: bool,
    pub autostart: bool,
    pub context_menu: bool,
    pub double_click_preview: bool,
    pub separate_notes: bool,
    pub tray_icon: bool,
    pub check_updates: bool,
    pub theme: ThemeMode,
    pub layout: Layout,
    pub transparency: u8,
    #[serde(deserialize_with = "font_size")]
    pub font_size: u8,
    #[serde(deserialize_with = "blur")]
    pub blur: bool,
    pub window_animations: bool,
    pub interface_animations: bool,
    pub language: Option<Language>,
    pub pinned: bool,
    #[serde(deserialize_with = "shake")]
    pub shake: ShakeLevel,
    pub corner: Option<Corner>,
    pub corner_drag: bool,
    pub near_cursor: bool,
    pub tile_input: TileInput,
    pub position: Option<(i32, i32)>,
    pub size: Option<(f32, f32)>,
    pub shortcuts: Shortcuts,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            automatic: true,
            autostart: !crate::profile::current().portable(),
            context_menu: !crate::profile::current().portable(),
            double_click_preview: true,
            separate_notes: true,
            tray_icon: true,
            check_updates: !crate::profile::current().testing(),
            theme: ThemeMode::default(),
            layout: Layout::default(),
            transparency: 0,
            font_size: DEFAULT_FONT_SIZE,
            blur: false,
            window_animations: false,
            interface_animations: false,
            language: None,
            pinned: false,
            shake: ShakeLevel::Off,
            corner: None,
            corner_drag: false,
            near_cursor: false,
            tile_input: TileInput::default(),
            position: None,
            size: None,
            shortcuts: Shortcuts::default(),
        }
    }
}

impl Settings {
    pub fn opacity(&self) -> f32 {
        1. - self.transparency.min(MAX_TRANSPARENCY) as f32 / 100.
    }

    pub fn blurred(&self) -> bool {
        self.blur && !self.window_animations
    }

    pub fn backdrop(&self) -> WindowBackgroundAppearance {
        if self.blurred() && self.opacity() < 1. {
            WindowBackgroundAppearance::LightBlurred
        } else {
            WindowBackgroundAppearance::Opaque
        }
    }

    pub fn shows_at_once(&self) -> bool {
        self.automatic && self.shake == ShakeLevel::Off
    }

    pub fn corner_drag_on(&self) -> bool {
        self.corner_drag && !self.shows_at_once()
    }

    pub fn set_shake(&mut self, level: ShakeLevel) {
        if self.automatic {
            self.shake = level;
        }
    }

    pub fn toggle_corner_drag(&mut self) {
        if self.corner.is_some() && !self.shows_at_once() {
            self.corner_drag = !self.corner_drag;
        }
    }

    pub fn toggle_near_cursor(&mut self) {
        if self.automatic {
            self.near_cursor = !self.near_cursor;
        }
    }

    pub fn toggle_blur(&mut self) {
        if !self.window_animations {
            self.blur = !self.blur;
        }
    }

    pub fn window_size(&self) -> (f32, f32) {
        let (width, height) = self.size.unwrap_or(DEFAULT_SIZE);
        (width.clamp(MIN_SIZE.0, MAX_SIZE), height.clamp(MIN_SIZE.1, MAX_SIZE))
    }
}

fn blur<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Bool(blur) => blur,
        serde_json::Value::String(level) => level != "Off",
        _ => false,
    })
}

fn font_size<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    let size = serde_json::from_value(serde_json::Value::deserialize(deserializer)?).unwrap_or(DEFAULT_FONT_SIZE);
    Ok(if FONT_SIZES.contains(&size) { size } else { DEFAULT_FONT_SIZE })
}

fn shake<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ShakeLevel, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Bool(true) => ShakeLevel::Low,
        value => serde_json::from_value(value).unwrap_or_default(),
    })
}
