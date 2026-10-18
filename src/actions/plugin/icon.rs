use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;
use std::sync::Arc;

const FILE_LIMIT: u64 = 1024 * 1024;
const SVG_NS: &str = "http://www.w3.org/2000/svg";

pub(super) fn load(path: &Path) -> Result<Arc<str>> {
    let size = fs::metadata(path)?.len();
    ensure!(size <= FILE_LIMIT, "the icon is larger than {} KB", FILE_LIMIT >> 10);
    let tree = usvg::Tree::from_data(&fs::read(path)?, &usvg::Options::default())?;
    ensure!(tree.root().has_children(), "the icon is empty");
    let (width, height) = (tree.size().width(), tree.size().height());
    let side = width.max(height);
    let inner = tree.to_string(&usvg::WriteOptions::default());
    let (x, y) = ((width - side) / 2., (height - side) / 2.);
    Ok(Arc::from(format!(
        "<svg xmlns='{SVG_NS}' width='{side}' height='{side}' viewBox='{x} {y} {side} {side}'>{inner}</svg>"
    )))
}

#[cfg(test)]
mod tests;
