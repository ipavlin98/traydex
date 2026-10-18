use super::{Hovered, Shelf};

#[derive(Default)]
pub struct Hover {
    target: Option<Hovered>,
    keyboard: bool,
}

impl Hover {
    pub fn target(&self) -> Option<&Hovered> {
        self.target.as_ref()
    }

    pub fn keyboard(&self) -> bool {
        self.keyboard
    }

    pub fn is(&self, target: &Hovered) -> bool {
        self.target.as_ref() == Some(target)
    }

    pub fn lit(&self, target: &Hovered) -> bool {
        self.keyboard && self.is(target)
    }

    fn set(&mut self, target: Hovered, hovered: bool) {
        if hovered {
            self.target = Some(target);
        } else if self.is(&target) {
            self.target = None;
        }
    }

    pub(super) fn step_to(&mut self, target: Hovered) {
        self.target = Some(target);
        self.keyboard = true;
    }

    pub(super) fn point(&mut self, target: Hovered) -> bool {
        if !self.keyboard && self.is(&target) {
            return false;
        }
        self.target = Some(target);
        std::mem::take(&mut self.keyboard)
    }

    pub(super) fn moved(&mut self) -> bool {
        if !std::mem::take(&mut self.keyboard) {
            return false;
        }
        if matches!(self.target, Some(Hovered::Item(_))) {
            self.target = None;
        }
        true
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

impl Shelf {
    pub(crate) fn hovered(&self) -> &Hover {
        &self.hover
    }

    pub fn hover(&mut self, target: Hovered, hovered: bool) {
        self.hover.set(target, hovered);
    }
}
