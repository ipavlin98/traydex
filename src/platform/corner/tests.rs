use super::*;

const REARM_PX: i32 = 60;

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT { left, top, right, bottom }
}

fn point(x: i32, y: i32) -> POINT {
    POINT { x, y }
}

fn pair(tip: (POINT, POINT)) -> ((i32, i32), (i32, i32)) {
    ((tip.0.x, tip.0.y), (tip.1.x, tip.1.y))
}

fn unused() -> bool {
    panic!("must not be evaluated")
}

fn fire(armed: &mut bool, distance: i32) -> bool {
    decide(armed, distance, REARM_PX, || false, || true, || false, || false)
}

#[test]
fn tips_are_the_outermost_pixels() {
    let screen = rect(0, 0, 1920, 1080);
    assert_eq!(pair(tip(Corner::TopLeft, screen)), ((0, 0), (-1, -1)));
    assert_eq!(pair(tip(Corner::TopRight, screen)), ((1919, 0), (1, -1)));
    assert_eq!(pair(tip(Corner::BottomLeft, screen)), ((0, 1079), (-1, 1)));
    assert_eq!(pair(tip(Corner::BottomRight, screen)), ((1919, 1079), (1, 1)));
    let left_monitor = rect(-1280, -200, 0, 824);
    assert_eq!(pair(tip(Corner::TopRight, left_monitor)), ((-1, -200), (1, -1)));
    assert_eq!(pair(tip(Corner::BottomLeft, left_monitor)), ((-1280, 823), (-1, 1)));
}

#[test]
fn distance_is_the_larger_axis_offset() {
    assert_eq!(distance(point(0, 0), point(0, 0)), 0);
    assert_eq!(distance(point(3, -10), point(0, 0)), 10);
    assert_eq!(distance(point(-1290, 5), point(-1280, 0)), 10);
}

#[test]
fn reaching_the_corner_fires_once() {
    let mut armed = true;
    assert!(!fire(&mut armed, REACH));
    assert!(armed);
    assert!(fire(&mut armed, REACH - 1));
    assert!(!armed);
    assert!(!fire(&mut armed, 0));
    assert!(!fire(&mut armed, REARM_PX));
    assert!(!armed);
}

#[test]
fn leaving_far_enough_rearms() {
    let mut armed = false;
    assert!(!fire(&mut armed, REARM_PX + 1));
    assert!(armed);
    assert!(fire(&mut armed, 0));
}

#[test]
fn entering_the_window_rearms() {
    let mut armed = false;
    assert!(!decide(&mut armed, 5, REARM_PX, || true, unused, unused, unused));
    assert!(armed);
}

#[test]
fn window_opened_under_the_cursor_in_the_corner_does_not_rearm_it() {
    let mut armed = false;
    assert!(!decide(&mut armed, 0, REARM_PX, || true, unused, unused, unused));
    assert!(!decide(&mut armed, REACH - 1, REARM_PX, || true, unused, unused, unused));
    assert!(!armed, "the window under the corner must not toggle it back");
    assert!(!decide(&mut armed, REACH, REARM_PX, || true, unused, unused, unused));
    assert!(armed, "stepping off the corner into the window rearms it");
}

#[test]
fn resize_grip_yields_exactly_the_pixels_where_the_corner_fires() {
    let screen = rect(0, 0, 1920, 1080);
    let cases = [
        (Corner::TopLeft, point(0, 0), (1, 1)),
        (Corner::TopRight, point(1919, 0), (-1, 1)),
        (Corner::BottomLeft, point(0, 1079), (1, -1)),
        (Corner::BottomRight, point(1919, 1079), (-1, -1)),
    ];
    for (corner, tip, (inward_x, inward_y)) in cases {
        for step in 0..=REACH + 1 {
            let at = point(tip.x + step * inward_x, tip.y + step * inward_y);
            let fires = fire(&mut true, step);
            assert_eq!(on_tip(at, corner, screen, |_, _| true), fires, "{corner:?}, {step} px inside");
        }
        assert!(!on_tip(tip, corner, screen, |_, _| false), "{corner:?} shared with another monitor keeps the grip");
    }
}

#[test]
fn corner_shared_with_another_monitor_is_ignored() {
    let mut armed = true;
    assert!(!decide(&mut armed, 0, REARM_PX, unused, || false, unused, unused));
    assert!(armed);
}

#[test]
fn held_button_or_fullscreen_window_disarm_without_firing() {
    let mut armed = true;
    assert!(!decide(&mut armed, 0, REARM_PX, unused, || true, || true, unused));
    assert!(!armed);
    armed = true;
    assert!(!decide(&mut armed, 0, REARM_PX, unused, || true, || false, || true));
    assert!(!armed);
}

#[test]
fn drag_reveals_the_window_anywhere_in_the_square_by_the_corner() {
    let zone = 440;
    assert!(reveals_drag(0, zone, || true, || true, || false));
    assert!(reveals_drag(zone - 1, zone, || true, || true, || false), "the far edge of the square");
    assert!(!reveals_drag(zone, zone, unused, unused, unused), "just outside the square");
}

#[test]
fn drag_area_needs_an_unseen_drag_on_an_outer_corner_without_a_full_screen_window() {
    assert!(!reveals_drag(10, 440, || false, unused, unused), "no drag or the window already shows it");
    assert!(!reveals_drag(10, 440, || true, || false, unused), "a corner shared with another monitor");
    assert!(!reveals_drag(10, 440, || true, || true, || true), "a full-screen window is in front");
}

#[test]
fn checks_run_lazily_in_order() {
    let mut armed = true;
    assert!(!decide(&mut armed, REACH + 10, REARM_PX, unused, unused, unused, unused));
    let mut armed = false;
    assert!(!decide(&mut armed, REARM_PX + 1, REARM_PX, unused, unused, unused, unused));
}

#[test]
fn chosen_corner_is_stored_and_cleared() {
    for corner in Corner::ALL {
        set_corner(Some(corner));
        assert_eq!(active(), Some(corner));
    }
    set_corner(None);
    assert_eq!(active(), None);
}

#[test]
fn saved_corner_names_are_stable() {
    let names: Vec<String> = Corner::ALL.iter().map(|corner| serde_json::to_string(corner).expect("json")).collect();
    assert_eq!(names, ["\"TopLeft\"", "\"TopRight\"", "\"BottomLeft\"", "\"BottomRight\""]);
}
