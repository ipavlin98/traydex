pub mod text {
    pub const CAPTION: f32 = 11.;
    pub const SMALL: f32 = 12.;
    pub const BODY: f32 = 13.;
    pub const TITLE: f32 = 15.;
}

pub mod line {
    pub const CAPTION: f32 = 14.;
    pub const SMALL: f32 = 16.;
    pub const BODY: f32 = 18.;
    pub const TITLE: f32 = 20.;
}

pub mod space {
    pub const X2S: f32 = 2.;
    pub const XS: f32 = 4.;
    pub const S: f32 = 6.;
    pub const M: f32 = 8.;
    pub const L: f32 = 10.;
    pub const XL: f32 = 12.;
    pub const X2L: f32 = 14.;
    pub const X3L: f32 = 16.;
    pub const X4L: f32 = 18.;
    pub const APART: f32 = 32.;

    pub const SUBTITLE: f32 = 0.;
    pub const LABEL: f32 = S;
    pub const GROUP: f32 = XL;
    pub const PAGE: f32 = X2L;
    pub const BLOCK: f32 = X2L;
    pub const SECTION: f32 = X4L;
}

pub mod radius {
    pub const SMALL: f32 = 4.;
    pub const CONTROL: f32 = 6.;
    pub const LARGE: f32 = 8.;
    pub const BADGE_SHARE: f32 = 1. / 4.5;
}

pub mod glyph {
    pub const XS: f32 = 12.;
    pub const S: f32 = 14.;
    pub const M: f32 = 16.;
    pub const L: f32 = 18.;
    pub const XL: f32 = 20.;
    pub const COVER: f32 = 40.;
    pub const BADGE_SHARE: f32 = 0.5;
}

pub mod size {
    pub const HAIRLINE: f32 = 1.;
    pub const KEY: f32 = 20.;
    pub const BUTTON: f32 = 24.;
    pub const CONTROL: f32 = 28.;
    pub const CHIP: f32 = 32.;
    pub const ROW: f32 = 50.;
    pub const ROW_BADGE: f32 = 34.;
    pub const PAGE_BADGE: f32 = 40.;
}

pub mod opacity {
    pub const DISABLED: f32 = 0.5;
    pub const UNMATCHED: f32 = 0.3;
}

pub mod timing {
    use std::time::Duration;

    pub const SPIN: Duration = Duration::from_millis(900);
}

pub const BASE: f32 = 16.;

pub fn rem(design: f32) -> gpui::Rems {
    gpui::rems(design / BASE)
}

pub fn scaled(design: f32, window: &gpui::Window) -> gpui::Pixels {
    rem(design).to_pixels(window.rem_size())
}

pub fn units(pixels: gpui::Pixels, window: &gpui::Window) -> f32 {
    f32::from(pixels) * BASE / f32::from(window.rem_size())
}

pub fn scale(window: &gpui::Window) -> f32 {
    window.scale_factor() * f32::from(window.rem_size()) / BASE
}

pub fn zoom(scale: u16) -> f32 {
    f32::from(scale) / 100.
}

pub fn device(logical: f32, scale: f32) -> f32 {
    (logical * scale).round()
}

pub fn snap(logical: f32, scale: f32) -> f32 {
    device(logical, scale) / scale
}

pub fn snap_up(logical: f32, scale: f32) -> f32 {
    (logical * scale).ceil() / scale
}

#[derive(Clone, Copy)]
pub struct Raster {
    pub scale: f32,
    pub factor: f32,
}

impl Raster {
    pub fn of(window: &gpui::Window) -> Self {
        Self { scale: scale(window), factor: window.scale_factor() }
    }

    pub fn rem(self, design: f32) -> f32 {
        device(design, self.scale)
    }

    pub fn px(self, logical: f32) -> f32 {
        (logical * self.factor).round()
    }

    pub fn text(self, design: f32) -> f32 {
        design * self.scale
    }
}

#[cfg(test)]
mod tests;
