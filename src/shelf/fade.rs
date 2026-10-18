use gpui::*;
use std::time::{Duration, Instant};

const FADE_IN: Duration = Duration::from_millis(220);
const FADE_OUT: Duration = Duration::from_millis(120);
const FADE_IN_CURVE: i32 = 2;
const FADE_OUT_CURVE: i32 = 3;

pub struct FadedOut;

pub struct Fade {
    from: f32,
    to: f32,
    start: Option<Instant>,
    duration: Duration,
    hiding: bool,
    task: Option<Task<()>>,
}

impl Default for Fade {
    fn default() -> Self {
        Self::settled(1.)
    }
}

impl EventEmitter<FadedOut> for Fade {}

fn ease_out(progress: f32, power: i32) -> f32 {
    1. - (1. - progress).powi(power)
}

impl Fade {
    pub fn out() -> Self {
        Self::settled(0.)
    }

    fn settled(level: f32) -> Self {
        Self { from: level, to: level, start: None, duration: Duration::ZERO, hiding: false, task: None }
    }

    pub fn level(&self, now: Instant) -> f32 {
        let Some(start) = self.start.filter(|_| self.task.is_some()) else {
            return self.to;
        };
        let progress = now.saturating_duration_since(start).as_secs_f32() / self.duration.as_secs_f32();
        let power = if self.hiding { FADE_OUT_CURVE } else { FADE_IN_CURVE };
        self.from + (self.to - self.from) * ease_out(progress.min(1.), power)
    }

    pub fn animating(&self) -> bool {
        self.task.is_some()
    }

    pub(super) fn hiding(&self) -> bool {
        self.hiding
    }

    pub(super) fn fade_in(&mut self, from_zero: bool, cx: &mut Context<Self>) {
        if from_zero {
            *self = Self::out();
        }
        self.hiding = false;
        self.run(1., FADE_IN, cx);
    }

    pub(super) fn fade_out(&mut self, cx: &mut Context<Self>) {
        if !self.hiding {
            self.hiding = true;
            self.run(0., FADE_OUT, cx);
        }
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) {
        *self = Self::default();
        cx.notify();
    }

    fn run(&mut self, target: f32, duration: Duration, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        self.from = self.level(now);
        self.to = target;
        self.start = Some(now);
        self.duration = duration;
        self.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            let _ = this.update(cx, |fade, cx| {
                fade.task = None;
                fade.from = fade.to;
                cx.notify();
                if fade.hiding {
                    cx.emit(FadedOut);
                }
            });
        }));
        cx.notify();
    }
}
