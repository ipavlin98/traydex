use crate::{AssetSource, DevicePixels, IsZero, Result, SharedString, Size};
use resvg::tiny_skia::Pixmap;
use std::{
    hash::Hash,
    sync::{Arc, LazyLock},
};

mod hint;

/// When rendering SVGs, we render them at twice the size to get a higher-quality result.
pub const SMOOTH_SVG_SCALE_FACTOR: f32 = 2.;

#[derive(Clone, PartialEq, Hash, Eq)]
pub(crate) struct RenderSvgParams {
    pub(crate) path: SharedString,
    pub(crate) size: Size<DevicePixels>,
}

#[derive(Clone)]
pub struct SvgRenderer {
    asset_source: Arc<dyn AssetSource>,
    usvg_options: Arc<usvg::Options<'static>>,
}

pub enum SvgSize {
    Size(Size<DevicePixels>),
    ScaleFactor(f32),
}

impl SvgRenderer {
    pub fn new(asset_source: Arc<dyn AssetSource>) -> Self {
        static FONT_DB: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        });
        let default_font_resolver = usvg::FontResolver::default_font_selector();
        let font_resolver = Box::new(
            move |font: &usvg::Font, db: &mut Arc<usvg::fontdb::Database>| {
                if db.is_empty() {
                    *db = FONT_DB.clone();
                }
                default_font_resolver(font, db)
            },
        );
        let options = usvg::Options {
            font_resolver: usvg::FontResolver {
                select_font: font_resolver,
                select_fallback: usvg::FontResolver::default_fallback_selector(),
            },
            ..Default::default()
        };
        Self {
            asset_source,
            usvg_options: Arc::new(options),
        }
    }

    pub(crate) fn render(
        &self,
        params: &RenderSvgParams,
    ) -> Result<Option<(Size<DevicePixels>, Vec<u8>)>> {
        anyhow::ensure!(!params.size.is_zero(), "can't render at a zero size");

        // Load the tree.
        let Some(bytes) = self.asset_source.load(&params.path)? else {
            return Ok(None);
        };

        let tree = usvg::Tree::from_data(&bytes, &self.usvg_options)?;
        Ok(Some(icon_mask(&tree, params.size)?))
    }

    pub fn render_pixmap(&self, bytes: &[u8], size: SvgSize) -> Result<Pixmap, usvg::Error> {
        let tree = usvg::Tree::from_data(bytes, &self.usvg_options)?;
        let scale = match size {
            SvgSize::Size(size) => size.width.0 as f32 / tree.size().width(),
            SvgSize::ScaleFactor(scale) => scale,
        };
        let mut pixmap = blank(&tree, scale)?;
        let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        Ok(pixmap)
    }
}

fn blank(tree: &usvg::Tree, scale: f32) -> Result<Pixmap, usvg::Error> {
    let svg_size = tree.size();
    Pixmap::new(
        (svg_size.width() * scale) as u32,
        (svg_size.height() * scale) as u32,
    )
    .ok_or(usvg::Error::InvalidSize)
}

/// Alpha mask of an icon drawn over `size` texels, `SMOOTH_SVG_SCALE_FACTOR` per device pixel,
/// with its straight edges fitted to whole device pixels (see `hint`).
pub fn svg_icon_mask(bytes: &[u8], size: Size<DevicePixels>) -> Result<(Size<DevicePixels>, Vec<u8>)> {
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default())?;
    icon_mask(&tree, size)
}

fn icon_mask(tree: &usvg::Tree, size: Size<DevicePixels>) -> Result<(Size<DevicePixels>, Vec<u8>)> {
    let scale = size.width.0 as f32 / tree.size().width();
    let mut pixmap = blank(tree, scale)?;
    let device = scale / SMOOTH_SVG_SCALE_FACTOR;
    if !hint::render(tree, device, SMOOTH_SVG_SCALE_FACTOR, &mut pixmap) {
        let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(tree, transform, &mut pixmap.as_mut());
    }
    let size = Size::new(
        DevicePixels(pixmap.width() as i32),
        DevicePixels(pixmap.height() as i32),
    );
    Ok((size, pixmap.pixels().iter().map(|p| p.alpha()).collect()))
}
