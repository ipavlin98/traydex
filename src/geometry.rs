use crate::design::space;
use crate::store::{Layout, MIN_SIZE};

pub const TITLE_HEIGHT: f32 = 41.;
pub const STRIP_PADDING: f32 = space::PAGE;
pub const CARD_SIZE: f32 = 164.;
pub const CARD_GAP: f32 = space::M;
pub const GUTTER: f32 = space::PAGE;
pub const TILE_GAP: f32 = space::X2L;
pub const MAX_COLUMNS: usize = 4;
const MAX_ROWS: usize = 9;
pub const COMPACT_MIN: f32 = 140.;
pub const COMPACT_HEIGHT: f32 = 44.;
pub const COMPACT_ZONE_HEIGHT: f32 = 52.;

pub fn columns(room: f32, min: f32, fewest: usize) -> usize {
    (((room + TILE_GAP) / (min + TILE_GAP)).floor() as usize).clamp(fewest, MAX_COLUMNS)
}

pub fn block(count: usize, columns: usize, height: f32) -> f32 {
    let rows = count.div_ceil(columns) as f32;
    rows * height + (rows - 1.).max(0.) * TILE_GAP
}

pub fn space(height: f32) -> f32 {
    height - TITLE_HEIGHT - GUTTER * 2. - TILE_GAP
}

fn compact_columns(count: usize) -> usize {
    count.div_ceil(MAX_ROWS).clamp(columns(MIN_SIZE.0 - GUTTER * 2., COMPACT_MIN, 1), MAX_COLUMNS)
}

pub fn drop_width(count: usize) -> f32 {
    let columns = compact_columns(count) as f32;
    (GUTTER * 2. + columns * COMPACT_MIN + (columns - 1.) * TILE_GAP).max(MIN_SIZE.0).ceil()
}

pub fn required_height(count: usize) -> f32 {
    if count == 0 {
        return MIN_SIZE.1;
    }
    let needed = COMPACT_ZONE_HEIGHT + block(count, compact_columns(count), COMPACT_HEIGHT);
    (MIN_SIZE.1 + needed - space(MIN_SIZE.1)).max(MIN_SIZE.1).ceil()
}

pub fn required_width(layout: Layout) -> f32 {
    match layout {
        Layout::List => MIN_SIZE.0,
        Layout::Grid => (STRIP_PADDING * 2. + CARD_SIZE * 2. + CARD_GAP).max(MIN_SIZE.0),
    }
}

pub fn least_size(layout: Layout, tiles: usize) -> (f32, f32) {
    (required_width(layout).max(drop_width(tiles)), required_height(tiles))
}
