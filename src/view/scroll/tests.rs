use super::*;

const MAX: f32 = 400.;
const NOTCH: f32 = 60.;
const FAR: f32 = 1_000.;
const SPEED: f32 = MIN_SPEED;
const DISPLAY_FRAME: Duration = Duration::from_millis(16);

fn settle(glide: &mut Glide, mut offset: Pixels, frame: Duration) -> (Pixels, u32) {
    let start = Instant::now();
    let mut frames = 0;
    while let Some(next) = glide.step(offset, start + frame * frames, SPEED) {
        offset = next;
        frames += 1;
        assert!(frames < 1_000, "the glide never settles");
    }
    (offset, frames)
}

#[test]
fn wheel_notches_add_up_and_stop_at_the_ends() {
    let mut glide = Glide::default();
    glide.wheel(px(0.), px(MAX), px(-NOTCH));
    glide.wheel(px(0.), px(MAX), px(-NOTCH));
    assert_eq!(glide.target, Some(px(-NOTCH * 2.)));
    let (offset, frames) = settle(&mut glide, px(0.), FIRST_FRAME);
    assert_eq!(offset, px(-NOTCH * 2.));
    assert!(frames > 1, "the list jumped instead of gliding");
    glide.wheel(offset, px(MAX), px(FAR));
    assert_eq!(glide.target, Some(px(0.)));
    glide.wheel(offset, px(MAX), px(-FAR));
    assert_eq!(glide.target, Some(px(-MAX)));
}

#[test]
fn a_glide_ends_without_a_slow_crawl() {
    let mut glide = Glide::default();
    glide.wheel(px(0.), px(MAX), px(-NOTCH));
    let (offset, frames) = settle(&mut glide, px(0.), DISPLAY_FRAME);
    assert_eq!(offset, px(-NOTCH));
    assert!(frames <= 8, "the last pixels crawled for {frames} frames");
}

#[test]
fn an_outside_jump_cancels_the_glide() {
    let mut glide = Glide::default();
    glide.wheel(px(0.), px(MAX), px(-NOTCH * 3.));
    let start = Instant::now();
    assert!(glide.step(px(0.), start, SPEED).is_some());
    assert_eq!(glide.step(px(0.), start + FIRST_FRAME, SPEED), None, "a key jump took over the list");
    assert!(!glide.moving());
}
