use super::*;

const AREA: Size<f32> = Size { width: 400., height: 300. };

fn stage() -> Stage {
    Stage::new(size(2000., 1000.), AREA, 20.)
}

fn image_point(zoom: &Zoom, at: Point<f32>) -> Point<f32> {
    let placed = zoom.place(stage());
    point((at.x - placed.origin.x) / placed.size.width, (at.y - placed.origin.y) / placed.size.height)
}

fn close(a: Point<f32>, b: Point<f32>) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}

#[test]
fn large_images_fit_inside_the_padding_and_small_ones_keep_their_size() {
    assert_eq!(stage().fitted, size(360., 180.));
    assert_eq!(Stage::new(size(40., 30.), AREA, 20.).fitted, size(40., 30.));
}

#[test]
fn unzoomed_image_is_centered() {
    let placed = Zoom::default().place(stage());
    assert_eq!(placed.origin, point(20., 60.));
}

#[test]
fn wheel_keeps_the_point_under_the_cursor() {
    let mut zoom = Zoom::default();
    let at = point(300., 150.);
    let before = image_point(&zoom, at);
    zoom.wheel(6., at, stage());
    assert!(zoom.zoomed());
    assert!(close(image_point(&zoom, at), before), "{:?} -> {:?}", before, image_point(&zoom, at));
    zoom.wheel(-1., at, stage());
    assert!(close(image_point(&zoom, at), before));
}

#[test]
fn zoom_never_goes_below_the_fitted_size() {
    let mut zoom = Zoom::default();
    zoom.wheel(4., point(10., 10.), stage());
    zoom.wheel(-40., point(390., 290.), stage());
    assert!(!zoom.zoomed());
    assert_eq!(zoom.place(stage()), Zoom::default().place(stage()));
}

#[test]
fn dragging_pans_but_never_uncovers_the_area() {
    let mut zoom = Zoom::default();
    zoom.wheel(10., point(200., 150.), stage());
    let start = zoom.place(stage()).origin;
    zoom.grab(point(200., 150.));
    assert!(zoom.drag(point(190., 145.), stage()));
    let moved = zoom.place(stage()).origin;
    assert!(close(moved, point(start.x - 10., start.y - 5.)), "{start:?} -> {moved:?}");
    assert!(zoom.drag(point(-5000., -5000.), stage()));
    let placed = zoom.place(stage());
    assert!(placed.origin.x <= 0. && placed.origin.x + placed.size.width >= AREA.width - 1e-3, "{placed:?}");
    assert!(placed.origin.y <= 0. && placed.origin.y + placed.size.height >= AREA.height - 1e-3, "{placed:?}");
    assert!(zoom.release());
    assert!(!zoom.drag(point(0., 0.), stage()));
}

#[test]
fn unzoomed_image_cannot_be_grabbed() {
    let mut zoom = Zoom::default();
    zoom.grab(point(100., 100.));
    assert!(!zoom.grabbed());
    assert!(!zoom.drag(point(0., 0.), stage()));
}

#[test]
fn double_click_zooms_in_at_the_cursor_and_back_to_fit() {
    let mut zoom = Zoom::default();
    let at = point(100., 120.);
    let before = image_point(&zoom, at);
    zoom.toggle(at, stage());
    assert!(zoom.zoomed());
    assert!(close(image_point(&zoom, at), before));
    zoom.toggle(at, stage());
    assert!(!zoom.zoomed());
    assert_eq!(zoom.place(stage()), Zoom::default().place(stage()));
}
