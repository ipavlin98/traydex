use super::*;
use crate::testing::Sandbox;

fn square(svg: &str) -> f32 {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("the result is valid svg");
    let size = tree.size();
    assert_eq!(size.width(), size.height(), "the icon is fitted into a square: {svg}");
    size.width()
}

#[test]
fn unreadable_icon_is_refused() {
    let sandbox = Sandbox::new();
    assert!(load(&sandbox.file("broken.svg", "<svg")).is_err());
    assert!(
        load(&sandbox.file("empty.svg", "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24'/>")).is_err()
    );
    assert!(load(&sandbox.join("missing.svg")).is_err());
}

#[test]
fn vector_icon_is_centered_in_a_square() {
    let sandbox = Sandbox::new();
    let tall = sandbox.file(
        "tall.svg",
        r##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="12" height="24"><rect width="12" height="24" fill="#3ba3e0"/></svg>"##,
    );
    let svg = load(&tall).expect("icon");
    assert!(svg.starts_with("<svg"), "the markup itself is the asset name: {svg}");
    assert_eq!(square(&svg), 24.);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("svg");
    let bounds = tree.root().abs_bounding_box();
    assert_eq!((bounds.left(), bounds.right()), (6., 18.), "the shape keeps its size and sits in the middle");
}
