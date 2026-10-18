use super::*;

const SPAN: i32 = 30;

fn press() -> Press {
    Press { thread: 7, source: None, shown: false, shake: Shake::default(), released: 0 }
}

const OFF: ShakeLevel = ShakeLevel::Off;
const LOW: ShakeLevel = ShakeLevel::Low;

fn poll(held: bool, automatic: bool, shake: ShakeLevel, now: Instant) -> Poll {
    Poll { held, exporting: false, automatic, shake, span: shake.span(), now }
}

fn never() -> Option<u32> {
    panic!("the drag source must not be probed")
}

fn no_cursor() -> i32 {
    panic!("the cursor must not be read")
}

#[test]
fn small_moves_do_not_count_as_shake() {
    let now = Instant::now();
    let mut shake = Shake::start(100, 3);
    for x in [110, 90, 129, 71] {
        assert!(!shake.feed(x, SPAN, now));
    }
    assert_eq!(shake, Shake::start(100, 3));
}

#[test]
fn three_quick_reversals_make_a_shake() {
    let start = Instant::now();
    let mut shake = Shake::start(0, 3);
    assert!(!shake.feed(40, SPAN, start));
    assert!(!shake.feed(0, SPAN, start + Duration::from_millis(100)));
    assert!(!shake.feed(40, SPAN, start + Duration::from_millis(200)));
    assert!(shake.feed(0, SPAN, start + Duration::from_millis(300)));
}

#[test]
fn moves_in_one_direction_never_shake() {
    let now = Instant::now();
    let mut shake = Shake::start(0, 3);
    for step in 1..20 {
        assert!(!shake.feed(step * 40, SPAN, now));
    }
}

#[test]
fn slow_reversals_restart_the_count() {
    let start = Instant::now();
    let mut shake = Shake::start(0, 3);
    shake.feed(40, SPAN, start);
    shake.feed(0, SPAN, start);
    shake.feed(40, SPAN, start + Duration::from_millis(500));
    assert!(!shake.feed(0, SPAN, start + Duration::from_millis(900)));
    assert_eq!(shake.turns, 1);
    assert!(!shake.feed(40, SPAN, start + Duration::from_millis(1000)));
    assert!(shake.feed(0, SPAN, start + Duration::from_millis(1100)));
}

#[test]
fn shake_span_scales_with_caller_dpi() {
    let now = Instant::now();
    let mut shake = Shake::start(0, 3);
    for x in [40, 0, 40, 0] {
        assert!(!shake.feed(x, 60, now));
    }
}

#[test]
fn idle_press_keeps_polling_without_signals() {
    let (next, signal) = step(press(), poll(true, true, OFF, Instant::now()), || None, no_cursor);
    assert_eq!(next, Some(press()));
    assert_eq!(signal, None);
}

#[test]
fn external_drag_shows_window_once_and_ends_on_drop() {
    let now = Instant::now();
    let (next, signal) = step(press(), poll(true, true, OFF, now), || Some(42), no_cursor);
    let next = next.expect("press");
    assert_eq!(signal, Some(Signal::Start));
    assert!(next.shown);
    assert_eq!(next.source, Some(42));
    let (again, signal) = step(next, poll(true, true, OFF, now), || Some(42), no_cursor);
    assert_eq!(signal, None);
    let (ended, signal) = step(again.expect("press"), poll(false, true, OFF, now), || None, no_cursor);
    assert_eq!(signal, Some(Signal::End));
    let ended = ended.expect("press");
    assert!(!ended.shown);
    assert_eq!(ended.source, None);
}

#[test]
fn cancelled_drag_ends_while_button_is_held() {
    let now = Instant::now();
    let (next, _) = step(press(), poll(true, true, OFF, now), || Some(42), no_cursor);
    let (_, signal) = step(next.expect("press"), poll(true, true, OFF, now), || None, no_cursor);
    assert_eq!(signal, Some(Signal::End));
}

#[test]
fn disabled_auto_show_still_reports_drag_end() {
    let now = Instant::now();
    let (next, signal) = step(press(), poll(true, false, OFF, now), || Some(42), no_cursor);
    assert_eq!(signal, None);
    let next = next.expect("press");
    assert!(!next.shown);
    let (_, signal) = step(next, poll(true, false, OFF, now), || None, no_cursor);
    assert_eq!(signal, Some(Signal::End));
}

#[test]
fn shake_mode_waits_for_a_shake() {
    let start = Instant::now();
    let positions = std::cell::Cell::new(0);
    let cursor = || positions.get();
    let (next, signal) = step(press(), poll(true, true, LOW, start), || Some(42), cursor);
    assert_eq!(signal, None);
    let mut current = next.expect("press");
    let mut signals = Vec::new();
    for (tick, x) in [40, 0, 40, 0, 40].into_iter().enumerate() {
        positions.set(x);
        let now = start + Duration::from_millis(50 * (tick as u64 + 1));
        let (next, signal) = step(current, poll(true, true, LOW, now), || Some(42), cursor);
        current = next.expect("press");
        signals.push(signal);
    }
    assert_eq!(signals, [None, None, None, Some(Signal::Start), None]);
    assert!(current.shown);
}

fn shake_shows_window(level: ShakeLevel, moves: &[i32]) -> bool {
    let start = Instant::now();
    let positions = std::cell::Cell::new(0);
    let cursor = || positions.get();
    let (next, _) = step(press(), poll(true, true, level, start), || Some(42), cursor);
    let mut current = next.expect("press");
    let mut shown = false;
    for (tick, &x) in moves.iter().enumerate() {
        positions.set(x);
        let now = start + Duration::from_millis(50 * (tick as u64 + 1));
        let (next, signal) = step(current, poll(true, true, level, now), || Some(42), cursor);
        current = next.expect("press");
        shown |= signal == Some(Signal::Start);
    }
    shown
}

#[test]
fn high_sensitivity_reacts_to_a_short_light_shake() {
    let light = [20, 0, 20];
    assert!(shake_shows_window(ShakeLevel::High, &light));
    assert!(!shake_shows_window(ShakeLevel::Low, &light));
    assert!(!shake_shows_window(ShakeLevel::High, &[10, 0, 10, 0, 10]));
    assert!(!shake_shows_window(ShakeLevel::High, &[20, 40, 60]));
}

#[test]
fn exporting_own_drag_is_ignored() {
    let mut exporting = poll(true, true, OFF, Instant::now());
    exporting.exporting = true;
    let (next, signal) = step(press(), exporting, never, no_cursor);
    assert_eq!(signal, None);
    assert_eq!(next.expect("press").source, None);
}

#[test]
fn polling_stops_after_release_ticks() {
    let now = Instant::now();
    let mut current = press();
    for _ in 1..RELEASE_TICKS {
        let (next, signal) = step(current, poll(false, true, OFF, now), || None, no_cursor);
        assert_eq!(signal, None);
        current = next.expect("still polling");
    }
    let (next, signal) = step(current, poll(false, true, OFF, now), never, no_cursor);
    assert_eq!(next, None);
    assert_eq!(signal, None);
}

#[test]
fn held_button_resets_release_count() {
    let now = Instant::now();
    let (next, _) = step(press(), poll(false, true, OFF, now), || None, no_cursor);
    let (next, _) = step(next.expect("press"), poll(false, true, OFF, now), || None, no_cursor);
    assert_eq!(next.expect("press").released, 2);
    let (next, _) = step(next.expect("press"), poll(true, true, OFF, now), || None, no_cursor);
    assert_eq!(next.expect("press").released, 0);
}

#[test]
fn release_during_drag_reports_end_before_stopping() {
    let now = Instant::now();
    let (next, _) = step(press(), poll(true, true, OFF, now), || Some(42), no_cursor);
    let mut current = next.expect("press");
    current.released = RELEASE_TICKS - 1;
    let (next, signal) = step(current, poll(false, true, OFF, now), never, no_cursor);
    assert_eq!(next, None);
    assert_eq!(signal, Some(Signal::End));
}

#[test]
fn right_button_cancels_only_an_ongoing_drag() {
    assert!(!cancels(None, false));
    assert!(!cancels(Some(press()), false));
    assert!(cancels(Some(Press { source: Some(42), ..press() }), false));
    assert!(cancels(None, true));
}

#[test]
fn right_press_is_swallowed_only_while_dragging_into_the_shown_window() {
    assert_eq!(right_press(false, true, true), RightPress::Pass);
    assert_eq!(right_press(false, false, false), RightPress::Pass);
    assert_eq!(right_press(true, false, true), RightPress::Cancel);
    assert_eq!(right_press(true, true, false), RightPress::Cancel);
    assert_eq!(right_press(true, true, true), RightPress::Swallow);
}
