use super::*;

fn load(name: &str) -> Option<String> {
    Assets.load(name).expect("load").map(|bytes| String::from_utf8(bytes.into_owned()).expect("utf8"))
}

#[test]
fn named_shapes_become_svg() {
    let svg = load("folder").expect("folder");
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    assert!(svg.contains("stroke='white'"));
    assert!(load("brand").expect("brand").contains("viewBox='8 8 48 48'"));
    assert!(load("no-such-icon").is_none());
    let inline = "<svg xmlns='http://www.w3.org/2000/svg'><path d='M1 1h2'/></svg>";
    assert_eq!(load(inline).as_deref(), Some(inline), "a plugin icon comes as its own markup");
    assert!(Assets.list("").expect("list").is_empty());
}

#[test]
fn progress_ring_reflects_percent() {
    let empty = load("ring-0").expect("ring");
    assert!(empty.contains("stroke-opacity='0'"));
    let full = load("ring-100").expect("ring");
    let over = load("ring-250").expect("ring");
    assert_eq!(full, over);
    assert!(!full.contains("stroke-opacity"));
    let half = load("ring-50").expect("ring");
    let length = std::f32::consts::TAU * RING_RADIUS;
    assert!(half.contains(&format!("stroke-dasharray='{:.2} {length:.2}'", length / 2.)));
    assert!(load("ring-track").expect("track").contains(&format!("r='{RING_RADIUS}'")));
    assert!(load("ring-").is_none());
    assert!(load("ring-x").is_none());
    assert!(load("ring--5").is_none());
}

#[test]
fn insets_are_defined_for_padded_icons() {
    assert_eq!(inset("unknown"), 0.);
    assert!(inset("folder") > 0. && inset("file") > inset("folder"));
}
