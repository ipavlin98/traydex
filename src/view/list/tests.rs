use super::selection_span;
use crate::design::Raster;

#[test]
fn the_selection_row_rounds_every_length_to_device_pixels_like_the_layout() {
    let raster = Raster { scale: 1.25, factor: 1.25 };
    assert_eq!(
        selection_span(&[(20., 6.4), (40., 6.4)], 60., raster),
        360.,
        "at 125% padding 14 takes 18 device pixels, gap 6 takes 8, gap 2 takes 3, and tabs, bar and margins add up to 360"
    );
}
