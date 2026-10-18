use super::{Outcome, Progress, extension};
use crate::i18n::{fill, tr};
use crate::store;
use crate::system::Services;
use anyhow::{Context, Result, bail};
use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use std::path::{Path, PathBuf};

const LARGEST: u32 = 4000;
const SMALL: u32 = 1200;

pub(super) fn readable(path: &Path) -> bool {
    store::is_image(path) || extension(path) == "pdf"
}

pub(super) fn recognize(
    files: &[PathBuf],
    language: &str,
    progress: &Progress,
    system: &dyn Services,
) -> Result<Outcome> {
    let sources: Vec<&PathBuf> = files.iter().filter(|file| file.is_file() && readable(file)).collect();
    if sources.is_empty() {
        bail!(tr().ocr_needs);
    }
    let mut texts = Vec::new();
    for (done, file) in sources.iter().enumerate() {
        if !progress.step(done, sources.len()) {
            break;
        }
        if extension(file) == "pdf" {
            system.render_pdf(file, &mut |page| {
                progress.ensure_running()?;
                texts.push(system.recognize(&legible(page), language)?);
                Ok(())
            })?;
        } else {
            let image = store::open_oriented(file).context(tr().image_read_failed)?;
            texts.push(system.recognize(&legible(image.into_rgba8()), language)?);
        }
    }
    let text =
        texts.iter().map(|text| text.trim()).filter(|text| !text.is_empty()).collect::<Vec<_>>().join("\r\n\r\n");
    if text.is_empty() {
        bail!(tr().ocr_nothing);
    }
    Ok(Outcome {
        message: fill(tr().ocr_copied, text.chars().count()),
        created: Vec::new(),
        clipboard: Some(text),
        show: None,
    })
}

fn legible(image: RgbaImage) -> RgbaImage {
    let image = DynamicImage::ImageRgb8(super::convert::flatten(&DynamicImage::ImageRgba8(image)));
    let side = image.width().max(image.height());
    let image = if side > LARGEST {
        image.resize(LARGEST, LARGEST, FilterType::Lanczos3)
    } else if side < SMALL {
        image.resize(side * 2, side * 2, FilterType::CatmullRom)
    } else {
        image
    };
    image.into_rgba8()
}
