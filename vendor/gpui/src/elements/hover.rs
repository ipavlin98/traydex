use std::time::{Duration, Instant};

use crate::{
    Background, Fill, Global, Hsla, LinearColorStop, Rgba, Style, Visibility, color::BackgroundTag,
};

/// A frame drawn now reaches the screen about one refresh later, so a transition starts
/// one frame in and the first frame after a hover change already shows movement.
const PRESENT_LEAD: Duration = Duration::from_millis(16);

/// Turns hover and group hover styles of stateful elements into timed transitions.
/// Without this global, or with no duration, hover styles apply instantly.
#[derive(Clone, Copy, Debug, Default)]
pub struct HoverTransition {
    /// How long a full transition between the resting and the hovered style takes.
    pub duration: Option<Duration>,
}

impl Global for HoverTransition {}

/// Progress of one element's transition towards or away from its hovered style.
#[derive(Clone, Copy, Debug)]
pub struct HoverFade {
    hovered: bool,
    from: f32,
    since: Instant,
}

impl HoverFade {
    /// A transition that has already settled in the given state.
    pub fn settled(hovered: bool, now: Instant) -> Self {
        Self {
            hovered,
            from: if hovered { 1. } else { 0. },
            since: now,
        }
    }

    /// Follows the current hover state and returns the progress towards the hovered style,
    /// from 0.0 (resting) to 1.0 (hovered).
    pub fn advance(&mut self, hovered: bool, now: Instant, duration: Duration) -> f32 {
        if hovered != self.hovered {
            self.from = self.progress(now, duration);
            self.hovered = hovered;
            self.since = now.checked_sub(PRESENT_LEAD).unwrap_or(now);
        }
        self.progress(now, duration)
    }

    /// The hover state this transition is heading to.
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// Whether the transition still has to move towards the current hover state,
    /// including the very first frame after the state changed.
    pub fn animating(&self, now: Instant, duration: Duration) -> bool {
        let progress = self.progress(now, duration);
        if self.hovered {
            progress < 1.
        } else {
            progress > 0.
        }
    }

    /// Progress towards the hovered style at `now`, without following a new hover state.
    /// Both directions decelerate into their target, and the time a transition takes is
    /// proportional to the distance left, so a reversed transition is not slower than a full one.
    pub fn progress(&self, now: Instant, duration: Duration) -> f32 {
        let target = if self.hovered { 1. } else { 0. };
        let distance = target - self.from;
        let span = duration.as_secs_f32() * distance.abs();
        if span <= f32::EPSILON {
            return target;
        }
        let elapsed = now.saturating_duration_since(self.since).as_secs_f32();
        let eased = 1. - (1. - (elapsed / span).min(1.)).powi(3);
        self.from + distance * eased
    }
}

/// Mixes the resting and the hovered style of an element. Colors are interpolated and the
/// rest switches halfway through. Opacity is interpolated only for elements without their own
/// background: opacity applies to every primitive separately, so a half transparent panel
/// would show the content beneath it through both its background and its children.
pub fn blend_hover(resting: &Style, hovered: &Style, progress: f32) -> Style {
    if progress <= 0. {
        return resting.clone();
    }
    if progress >= 1. {
        return hovered.clone();
    }
    let mut style = if progress < 0.5 {
        resting.clone()
    } else {
        hovered.clone()
    };
    style.background = mix_fill(resting.background.as_ref(), hovered.background.as_ref(), progress);
    style.border_color = mix_optional(resting.border_color, hovered.border_color, progress);
    if let (Some(from), Some(to)) = (resting.text.color, hovered.text.color) {
        style.text.color = Some(mix(from, to, progress));
    }
    let layered = resting.background.is_some() || hovered.background.is_some();
    if layered || (resting.visibility == Visibility::Hidden && hovered.visibility == Visibility::Hidden) {
        return style;
    }
    let shown = |style: &Style| {
        if style.visibility == Visibility::Visible {
            style.opacity.unwrap_or(1.)
        } else {
            0.
        }
    };
    let opacity = shown(resting) + (shown(hovered) - shown(resting)) * progress;
    style.visibility = Visibility::Visible;
    style.opacity = (opacity < 1.).then_some(opacity);
    style
}

fn mix(from: Hsla, to: Hsla, amount: f32) -> Hsla {
    let (from, to) = (Rgba::from(from), Rgba::from(to));
    let a = from.a + (to.a - from.a) * amount;
    let channel = |from_value: f32, to_value: f32| {
        if a <= f32::EPSILON {
            from_value + (to_value - from_value) * amount
        } else {
            (from_value * from.a + (to_value * to.a - from_value * from.a) * amount) / a
        }
    };
    Hsla::from(Rgba {
        r: channel(from.r, to.r),
        g: channel(from.g, to.g),
        b: channel(from.b, to.b),
        a,
    })
}

fn clear(color: Hsla) -> Hsla {
    color.opacity(0.)
}

fn mix_optional(from: Option<Hsla>, to: Option<Hsla>, amount: f32) -> Option<Hsla> {
    match (from, to) {
        (None, None) => None,
        (Some(from), Some(to)) => Some(mix(from, to, amount)),
        (None, Some(to)) => Some(mix(clear(to), to, amount)),
        (Some(from), None) => Some(mix(from, clear(from), amount)),
    }
}

fn background(fill: &Fill) -> Background {
    match fill {
        Fill::Color(background) => *background,
    }
}

fn stops(background: &Background) -> [LinearColorStop; 2] {
    match background.tag {
        BackgroundTag::LinearGradient => background.colors,
        _ => [
            LinearColorStop {
                color: background.solid,
                percentage: 0.,
            },
            LinearColorStop {
                color: background.solid,
                percentage: 1.,
            },
        ],
    }
}

fn mix_fill(from: Option<&Fill>, to: Option<&Fill>, amount: f32) -> Option<Fill> {
    let (from, to) = match (from.map(background), to.map(background)) {
        (None, None) => return None,
        (Some(from), Some(to)) => (from, to),
        (None, Some(to)) => (fade_out(to), to),
        (Some(from), None) => (from, fade_out(from)),
    };
    let solid = |background: &Background| background.tag == BackgroundTag::Solid;
    if solid(&from) && solid(&to) {
        let mut mixed = to;
        mixed.solid = mix(from.solid, to.solid, amount);
        return Some(Fill::Color(mixed));
    }
    if !matches!(from.tag, BackgroundTag::Solid | BackgroundTag::LinearGradient)
        || !matches!(to.tag, BackgroundTag::Solid | BackgroundTag::LinearGradient)
    {
        return Some(Fill::Color(if amount < 0.5 { from } else { to }));
    }
    let shape = if solid(&to) { from } else { to };
    let (from_stops, to_stops) = (stops(&from), stops(&to));
    let mut mixed = shape;
    mixed.colors = [0, 1].map(|index| LinearColorStop {
        color: mix(from_stops[index].color, to_stops[index].color, amount),
        percentage: from_stops[index].percentage
            + (to_stops[index].percentage - from_stops[index].percentage) * amount,
    });
    Some(Fill::Color(mixed))
}

fn fade_out(background: Background) -> Background {
    let mut faded = background;
    faded.solid = clear(faded.solid);
    for stop in &mut faded.colors {
        stop.color = clear(stop.color);
    }
    faded
}
