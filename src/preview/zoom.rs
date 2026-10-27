use gpui::{Bounds, Point, Size, point, size};

const MAX_SCALE: f32 = 16.;
const NOTCH: f32 = 1.25;
const DOUBLE_CLICK: f32 = 3.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    pub area: Size<f32>,
    pub fitted: Size<f32>,
}

impl Stage {
    pub fn new(natural: Size<f32>, area: Size<f32>, padding: f32) -> Self {
        let room = size(area.width - padding * 2., area.height - padding * 2.);
        let ratio = (room.width / natural.width).min(room.height / natural.height).clamp(0., 1.);
        Self { area, fitted: size(natural.width * ratio, natural.height * ratio) }
    }
}

#[derive(Debug)]
pub struct Zoom {
    scale: f32,
    offset: Point<f32>,
    grab: Option<Point<f32>>,
}

impl Default for Zoom {
    fn default() -> Self {
        Self { scale: 1., offset: Point::default(), grab: None }
    }
}

impl Zoom {
    pub fn zoomed(&self) -> bool {
        self.scale > 1.
    }

    pub fn grabbed(&self) -> bool {
        self.grab.is_some()
    }

    pub fn place(&self, stage: Stage) -> Bounds<f32> {
        let extent = size(stage.fitted.width * self.scale, stage.fitted.height * self.scale);
        let axis = |offset: f32, extent: f32, area: f32| {
            if extent <= area { (area - extent) / 2. } else { offset.clamp(area - extent, 0.) }
        };
        Bounds {
            origin: point(
                axis(self.offset.x, extent.width, stage.area.width),
                axis(self.offset.y, extent.height, stage.area.height),
            ),
            size: extent,
        }
    }

    pub fn wheel(&mut self, notches: f32, at: Point<f32>, stage: Stage) {
        self.scale_at(self.scale * NOTCH.powf(notches), at, stage);
    }

    pub fn toggle(&mut self, at: Point<f32>, stage: Stage) {
        if self.zoomed() {
            *self = Self::default();
        } else {
            self.scale_at(DOUBLE_CLICK, at, stage);
        }
    }

    pub fn grab(&mut self, at: Point<f32>) {
        if self.zoomed() {
            self.grab = Some(at);
        }
    }

    pub fn drag(&mut self, at: Point<f32>, stage: Stage) -> bool {
        let Some(from) = self.grab.replace(at) else {
            return false;
        };
        let origin = self.place(stage).origin;
        self.offset = point(origin.x + at.x - from.x, origin.y + at.y - from.y);
        true
    }

    pub fn release(&mut self) -> bool {
        self.grab.take().is_some()
    }

    fn scale_at(&mut self, scale: f32, at: Point<f32>, stage: Stage) {
        let scale = scale.clamp(1., MAX_SCALE);
        let origin = self.place(stage).origin;
        let ratio = scale / self.scale;
        self.offset = point(at.x - (at.x - origin.x) * ratio, at.y - (at.y - origin.y) * ratio);
        self.scale = scale;
        if !self.zoomed() {
            self.grab = None;
        }
    }
}

#[cfg(test)]
mod tests;
