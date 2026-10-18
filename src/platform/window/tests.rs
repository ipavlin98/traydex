use super::*;

const GRIP_PX: i32 = 6;
const CORNER_PX: i32 = 16;

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT { left, top, right, bottom }
}

fn sides(rect: RECT) -> (i32, i32, i32, i32) {
    (rect.left, rect.top, rect.right, rect.bottom)
}

fn code(left: i32, top: i32, right: i32, bottom: i32) -> Option<u32> {
    edge_code(left, top, right, bottom, GRIP_PX, CORNER_PX)
}

fn no_work(_: &RECT) -> Option<RECT> {
    None
}

#[test]
fn title_bar_drags_except_over_controls() {
    let title = 41;
    let leading = 40..120;
    let trailing = 400;
    assert_eq!(region(POINT { x: 10, y: 5 }, title, leading.clone(), trailing), HTCAPTION);
    assert_eq!(region(POINT { x: 200, y: 40 }, title, leading.clone(), trailing), HTCAPTION);
    assert_eq!(region(POINT { x: 60, y: 5 }, title, leading.clone(), trailing), HTCLIENT);
    assert_eq!(region(POINT { x: 120, y: 5 }, title, leading.clone(), trailing), HTCAPTION);
    assert_eq!(region(POINT { x: 400, y: 5 }, title, leading.clone(), trailing), HTCLIENT);
    assert_eq!(region(POINT { x: 200, y: 41 }, title, leading, trailing), HTCLIENT);
    assert_eq!(region(POINT { x: 10, y: 5 }, title, 0..0, trailing), HTCAPTION);
}

#[test]
fn corners_take_priority_over_edges() {
    assert_eq!(code(0, 0, 500, 500), Some(HTTOPLEFT));
    assert_eq!(code(15, 2, 485, 498), Some(HTTOPLEFT));
    assert_eq!(code(2, 15, 498, 485), Some(HTTOPLEFT));
    assert_eq!(code(500, 0, 0, 500), Some(HTTOPRIGHT));
    assert_eq!(code(0, 500, 500, 0), Some(HTBOTTOMLEFT));
    assert_eq!(code(500, 500, 0, 0), Some(HTBOTTOMRIGHT));
}

#[test]
fn top_and_bottom_edges_resize_vertically() {
    assert_eq!(code(200, 0, 300, 500), Some(HTTOP));
    assert_eq!(code(200, 5, 300, 495), Some(HTTOP));
    assert_eq!(code(200, 500, 300, 0), Some(HTBOTTOM));
    assert_eq!(code(16, 0, 484, 500), Some(HTTOP));
}

#[test]
fn side_edges_and_interior_are_not_frame() {
    assert_eq!(code(200, 6, 300, 494), None);
    assert_eq!(code(0, 200, 500, 300), None);
    assert_eq!(code(500, 200, 0, 300), None);
    for hit in [HTTOP, HTBOTTOM, HTTOPLEFT, HTTOPRIGHT, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION] {
        assert!(is_frame(hit));
    }
    assert!(!is_frame(HTLEFT) && !is_frame(HTRIGHT) && !is_frame(HTCLIENT));
}

#[test]
fn snapping_pulls_the_nearest_edge() {
    assert_eq!(edge(5, 405, 0, 1920, 10), -5);
    assert_eq!(edge(1500, 1912, 0, 1920, 10), 8);
    assert_eq!(edge(100, 500, 0, 1920, 10), 0);
    assert_eq!(edge(-10, 390, 0, 1920, 10), 10);
    assert_eq!(edge(3, 1918, 0, 1920, 10), -3);
}

#[test]
fn anchoring_keeps_the_side_closest_to_the_screen_edge() {
    assert_eq!(anchored(0, 400, 0, 1920, 600), 0);
    assert_eq!(anchored(1520, 1920, 0, 1920, 600), 1320);
    assert_eq!(anchored(1, 401, 0, 1920, 600), 1);
    assert_eq!(anchored(1300, 1700, 0, 1920, 600), 1100);
    assert_eq!(anchored(200, 600, 0, 1920, 600), 200);
}

#[test]
fn stretching_moves_only_dragged_sides() {
    let start = rect(100, 100, 600, 600);
    assert_eq!(sides(stretch(start, WMSZ_RIGHT, (50, 30), no_work, 10, (300, 300))), (100, 100, 650, 600));
    assert_eq!(sides(stretch(start, WMSZ_TOPLEFT, (-20, -40), no_work, 10, (300, 300))), (80, 60, 600, 600));
    assert_eq!(sides(stretch(start, WMSZ_BOTTOM, (99, 25), no_work, 10, (300, 300))), (100, 100, 600, 625));
}

#[test]
fn stretching_snaps_to_work_area() {
    let start = rect(100, 100, 600, 600);
    let work = |_: &RECT| Some(rect(0, 0, 1920, 1040));
    assert_eq!(sides(stretch(start, WMSZ_BOTTOMRIGHT, (1315, 432), work, 10, (300, 300))), (100, 100, 1920, 1040));
    assert_eq!(sides(stretch(start, WMSZ_TOPLEFT, (-92, -89), work, 10, (300, 300))), (0, 11, 600, 600));
}

#[test]
fn stretching_respects_minimum_size_from_the_dragged_side() {
    let start = rect(100, 100, 600, 600);
    assert_eq!(sides(stretch(start, WMSZ_LEFT, (450, 0), no_work, 10, (384, 360))), (216, 100, 600, 600));
    assert_eq!(sides(stretch(start, WMSZ_RIGHT, (-450, 0), no_work, 10, (384, 360))), (100, 100, 484, 600));
    assert_eq!(sides(stretch(start, WMSZ_TOP, (0, 450), no_work, 10, (384, 360))), (100, 240, 600, 600));
    assert_eq!(sides(stretch(start, WMSZ_BOTTOM, (0, -450), no_work, 10, (384, 360))), (100, 100, 600, 460));
}

#[test]
fn work_area_lookup_receives_the_stretched_rect() {
    let start = rect(0, 0, 400, 400);
    let seen = std::cell::Cell::new(None);
    stretch(
        start,
        WMSZ_RIGHT,
        (100, 0),
        |next: &RECT| {
            seen.set(Some(sides(*next)));
            None
        },
        10,
        (0, 0),
    );
    assert_eq!(seen.get(), Some((0, 0, 500, 400)));
}

#[test]
fn window_fits_when_mostly_on_screen() {
    let work = rect(0, 0, 1920, 1040);
    assert!(covers(&rect(100, 100, 600, 600), &work, 40));
    assert!(covers(&rect(1670, 100, 2170, 600), &work, 40));
    assert!(!covers(&rect(1671, 100, 2171, 600), &work, 40));
    assert!(covers(&rect(100, 1000, 600, 1500), &work, 40));
    assert!(!covers(&rect(100, 1001, 600, 1500), &work, 40));
    assert!(!covers(&rect(3000, 100, 3500, 600), &work, 40));
    assert!(covers(&rect(-1100, 100, -600, 600), &rect(-1280, 0, 0, 1024), 40));
}

#[test]
fn window_opens_beside_the_cursor_without_covering_it() {
    let work = rect(0, 0, 3840, 2120);
    let at = |x, y| POINT { x, y };
    assert_eq!(beside(at(1000, 1000), work, (540, 540), 32), (1032, 730));
    assert_eq!(beside(at(3600, 1000), work, (540, 540), 32), (3028, 730));
    assert_eq!(beside(at(1000, 100), work, (540, 540), 32), (1032, 0));
    assert_eq!(beside(at(1000, 2100), work, (540, 540), 32), (1032, 1580));
    assert_eq!(beside(at(-1000, 500), rect(-1280, 0, 0, 1024), (540, 540), 32), (-968, 230));
}

#[test]
fn narrow_screen_pushes_the_window_to_the_roomier_edge() {
    let work = rect(0, 0, 800, 600);
    let at = |x, y| POINT { x, y };
    assert_eq!(beside(at(300, 300), work, (540, 540), 32), (260, 30));
    assert_eq!(beside(at(500, 300), work, (540, 540), 32), (0, 30));
    assert_eq!(beside(at(100, 300), rect(0, 0, 400, 300), (540, 540), 32), (0, 0));
}
