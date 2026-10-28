use crate::design::scaled;
use gpui::{
    DispatchPhase, ElementId, HitboxBehavior, IntoElement, Pixels, ScrollHandle, ScrollWheelEvent, Styled, canvas,
    point, px,
};
use std::time::{Duration, Instant};

const SETTLE: Duration = Duration::from_millis(40);
const FIRST_FRAME: Duration = Duration::from_millis(8);
const LONGEST_FRAME: Duration = Duration::from_millis(50);
const SETTLE_EPSILON: f32 = 0.5;
const MIN_SPEED: f32 = 320.;

pub(super) fn glide(id: impl Into<ElementId>, handle: ScrollHandle) -> impl IntoElement {
    let id = id.into();
    let listened = handle.clone();
    canvas(
        move |bounds, window, cx| {
            let state = window.use_keyed_state(id, cx, |_, _| Glide::default());
            let speed = f32::from(scaled(MIN_SPEED, window));
            let now = cx.background_executor().now();
            let offset = handle.offset();
            let (next, moving) = state.update(cx, |glide, _| (glide.step(offset.y, now, speed), glide.moving()));
            if let Some(next) = next {
                handle.set_offset(point(offset.x, next));
            }
            if moving {
                window.request_animation_frame();
            }
            (window.insert_hitbox(bounds, HitboxBehavior::Normal), state)
        },
        move |_, (hitbox, state), window, _| {
            let view = window.current_view();
            let line = window.line_height();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                let max = listened.max_offset().height;
                if phase != DispatchPhase::Capture || !hitbox.should_handle_scroll(window) || max <= px(0.) {
                    return;
                }
                cx.stop_propagation();
                let (current, delta) = (listened.offset().y, event.delta.pixel_delta(line).y);
                state.update(cx, |glide, _| glide.wheel(current, max, delta));
                cx.notify(view);
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[derive(Default)]
struct Glide {
    target: Option<Pixels>,
    written: Option<Pixels>,
    tick: Option<Instant>,
}

impl Glide {
    fn moving(&self) -> bool {
        self.target.is_some()
    }

    fn wheel(&mut self, current: Pixels, max: Pixels, delta: Pixels) {
        let destination = self.target.filter(|_| self.written == Some(current)).unwrap_or(current);
        self.target = Some((destination + delta).clamp(-max, px(0.)));
        self.written = Some(current);
    }

    fn step(&mut self, current: Pixels, now: Instant, speed: f32) -> Option<Pixels> {
        let Some(target) = self.target.filter(|_| self.written == Some(current)) else {
            self.target = None;
            self.tick = None;
            return None;
        };
        let elapsed = self.tick.map_or(FIRST_FRAME, |tick| now.saturating_duration_since(tick)).min(LONGEST_FRAME);
        let share = 1. - (-elapsed.as_secs_f32() / SETTLE.as_secs_f32()).exp();
        let gap = f32::from(target - current);
        let least = (speed * elapsed.as_secs_f32()).max(SETTLE_EPSILON);
        let next =
            if gap.abs() <= least { target } else { current + px(gap.signum() * (gap * share).abs().max(least)) };
        self.written = Some(next);
        if next == target {
            self.target = None;
            self.tick = None;
        } else {
            self.tick = Some(now);
        }
        Some(next)
    }
}

#[cfg(test)]
mod tests;
