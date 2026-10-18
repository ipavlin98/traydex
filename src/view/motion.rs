use crate::design::rem;
use gpui::{prelude::*, *};
use std::time::Duration;

const ENTER: Duration = Duration::from_millis(180);
const SHIFT: f32 = 6.;

#[derive(Clone, Copy)]
pub(super) enum Motion {
    Fade,
    Rise,
    Drop,
}

impl Motion {
    fn shift(self) -> f32 {
        match self {
            Motion::Fade => 0.,
            Motion::Rise => SHIFT,
            Motion::Drop => -SHIFT,
        }
    }
}

pub(super) trait Enter: Styled + IntoElement + Sized + 'static {
    fn enter(self, id: impl Into<ElementId>, motion: Motion, animated: bool) -> AnyElement {
        if !animated {
            return self.into_any_element();
        }
        let shift = motion.shift();
        self.with_animation(id, Animation::new(ENTER).with_easing(ease_out_quint()), move |element, delta| {
            let element = element.opacity(delta);
            if shift == 0. { element } else { element.relative().top(rem(shift * (1. - delta))) }
        })
        .into_any_element()
    }
}

impl<E: Styled + IntoElement + 'static> Enter for E {}
