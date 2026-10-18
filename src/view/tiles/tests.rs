use super::{TILE_HEIGHT, TILE_MIN, ZONE_HEIGHT, layout, strip_span};
use crate::actions::MAX_TILES;
use crate::design::Raster;
use crate::geometry::{
    COMPACT_HEIGHT, COMPACT_MIN, COMPACT_ZONE_HEIGHT, MAX_COLUMNS, TILE_GAP, block, columns, drop_width,
    required_height, required_width, space,
};
use crate::store::Layout as Arrangement;
use crate::store::MIN_SIZE;
use crate::testing::NARROW_SIZE;

#[test]
fn columns_are_clamped() {
    assert_eq!(columns(100., TILE_MIN, 2), 2);
    assert_eq!(columns(356., TILE_MIN, 2), 3);
    assert_eq!(columns(5000., TILE_MIN, 2), MAX_COLUMNS);
    assert_eq!(columns(100., COMPACT_MIN, 1), 1);
}

#[test]
fn block_height_includes_gaps_between_rows() {
    assert_eq!(block(0, 2, TILE_HEIGHT), 0.);
    assert_eq!(block(1, 2, TILE_HEIGHT), TILE_HEIGHT);
    assert_eq!(block(3, 2, TILE_HEIGHT), TILE_HEIGHT * 2. + TILE_GAP);
    assert_eq!(block(8, 4, COMPACT_HEIGHT), COMPACT_HEIGHT * 2. + TILE_GAP);
}

#[test]
fn roomy_window_uses_regular_tiles() {
    let roomy = layout(540., 540., 4, true);
    assert!(!roomy.compact);
    assert!(roomy.columns >= 2);
    assert!(roomy.zone.is_some_and(|zone| zone >= ZONE_HEIGHT));
    assert!(roomy.text >= 20.);
}

#[test]
fn crowded_window_switches_to_compact_tiles() {
    let crowded = layout(NARROW_SIZE.0, NARROW_SIZE.1, 12, true);
    assert!(crowded.compact);
    assert!(crowded.zone.is_some_and(|zone| zone >= COMPACT_ZONE_HEIGHT));
    let narrow = layout(10., 100., 3, true);
    assert_eq!(narrow.columns, 1);
    assert_eq!(narrow.text, 20.);
}

fn fits(width: f32, height: f32, count: usize) -> bool {
    let fitted = layout(width, height, count, true);
    let tile = if fitted.compact { COMPACT_HEIGHT } else { TILE_HEIGHT };
    fitted
        .zone
        .is_some_and(|zone| zone >= COMPACT_ZONE_HEIGHT && zone + block(count, fitted.columns, tile) <= space(height))
}

#[test]
fn without_the_zone_the_tiles_keep_its_room() {
    let with_zone = layout(NARROW_SIZE.0, 400., 8, true);
    let without = layout(NARROW_SIZE.0, 400., 8, false);
    assert!(with_zone.compact, "the zone leaves too little room for regular tiles");
    assert!(!without.compact && without.zone.is_none(), "the same window fits regular tiles without the zone");
}

#[test]
fn minimum_size_fits_every_tile_count() {
    assert_eq!((drop_width(0), required_height(0)), MIN_SIZE);
    let mut previous = 0.;
    for count in 0..60 {
        let (width, height) = (drop_width(count), required_height(count));
        assert!(width >= MIN_SIZE.0 && height >= MIN_SIZE.1, "{count}");
        assert!(width >= previous, "{count}: the window does not get narrower as tiles are added");
        assert_eq!((width, height), (width.ceil(), height.ceil()));
        assert!(fits(width, height, count), "{count} tiles overflow {width}×{height}");
        previous = width;
    }
}

#[test]
fn drop_screen_with_any_allowed_tile_count_fits_a_full_hd_screen_at_150_percent() {
    const WORK_AREA: (f32, f32) = (1920. / 1.5, (1080. - 72.) / 1.5);
    for count in 0..=MAX_TILES {
        let (width, height) = (drop_width(count), required_height(count));
        assert!(
            width <= WORK_AREA.0 && height <= WORK_AREA.1,
            "{count} tiles need {width}×{height}, the screen has {WORK_AREA:?}"
        );
    }
}

#[test]
fn minimum_width_fits_both_arrangements() {
    assert!(required_width(Arrangement::List) >= MIN_SIZE.0);
    assert!(required_width(Arrangement::Grid) >= required_width(Arrangement::List));
}

#[test]
fn filled_tiles_trade_columns_for_rows_instead_of_stretching() {
    assert_eq!(layout(675., 675., 8, false).columns, 3, "8 tiles in a square window: 3 per row, not 4 tall ones");
    assert_eq!(layout(540., 540., 4, false).columns, 2, "4 tiles in a square window: 2×2");
    assert_eq!(layout(540., 540., 1, false).columns, 1, "a single tile spans the whole width");
    assert_eq!(layout(675., 675., 8, true).columns, 4, "with the zone the grid stays as before");
}

#[test]
fn fewer_tiles_than_columns_stretch_across_the_drop_screen() {
    assert_eq!(layout(675., 675., 2, true).columns, 2, "two tiles share the whole row");
    assert_eq!(layout(675., 675., 1, true).columns, 1, "a single tile spans the whole width");
    assert_eq!(layout(NARROW_SIZE.0, NARROW_SIZE.1, 1, true).columns, 1, "compact tiles stretch too");
}

#[test]
fn the_strip_rounds_every_length_to_device_pixels_like_the_layout() {
    let raster = Raster { scale: 1.25, factor: 1.25 };
    let titles = [Some(27.2), Some(34.4), Some(36.)];
    let from = |first: usize| strip_span(titles.get(first..).unwrap_or_default(), raster);
    assert_eq!(from(0) - from(1), 100., "the first tile and its gap take 100 device pixels at 125%");
    assert_eq!(from(0) - from(2), 209., "the first two tiles and their gaps take 209 device pixels at 125%");
}
