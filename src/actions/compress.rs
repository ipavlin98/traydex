use super::{Compression, Outcome, Progress, Report, extension, output, stem};
use crate::i18n::tr;
use crate::store::{self, Storage, human_size};
use anyhow::{Context, Result, anyhow, bail};
use image::DynamicImage;
use image::imageops::FilterType;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const MAX_SIDE: u32 = 2560;
const QUALITY: f32 = 80.;
const COMPRESSIBLE: [&str; 5] = ["png", "jpg", "jpeg", "webp", "bmp"];

pub(super) fn compressible(path: &Path) -> bool {
    COMPRESSIBLE.contains(&extension(path).as_str())
}

pub(super) fn compress(
    files: &[PathBuf],
    format: Compression,
    progress: &Progress,
    storage: &Storage,
) -> Result<Outcome> {
    let images: Vec<&PathBuf> = files.iter().filter(|file| file.is_file() && compressible(file)).collect();
    if images.is_empty() {
        bail!(tr().compress_needs);
    }
    let mut report = Report { skipped: files.len() - images.len(), ..Report::default() };
    let (mut before, mut after, mut optimal) = (0, 0, 0);
    for (done, file) in images.iter().enumerate() {
        if !progress.step(done, images.len()) {
            break;
        }
        match compress_one(file, format, progress, storage) {
            Ok(Some((target, from, to))) => {
                report.created.push(target);
                before += from;
                after += to;
            }
            Ok(None) => {
                report.skipped += 1;
                optimal += 1;
            }
            Err(error) => report.fail(file, error),
        }
    }
    if report.created.is_empty() && report.error.is_none() && optimal > 0 {
        bail!(tr().already_compressed);
    }
    let mut outcome = report.finish(tr().compress_needs.into())?;
    outcome.message = format!("{} · {} → {}", outcome.message, human_size(before), human_size(after));
    Ok(outcome)
}

fn compress_one(
    file: &Path,
    format: Compression,
    progress: &Progress,
    storage: &Storage,
) -> Result<Option<(PathBuf, u64, u64)>> {
    let original = fs::metadata(file)?.len();
    let image = store::open_oriented(file).context(tr().image_read_failed)?;
    let image = if image.width().max(image.height()) > MAX_SIDE {
        image.resize(MAX_SIDE, MAX_SIDE, FilterType::Lanczos3)
    } else {
        image
    };
    progress.ensure_running()?;
    let (bytes, kind) = match format {
        Compression::Jpeg => (jpeg(&image)?, "jpg"),
        Compression::Webp => (webp(&image)?, "webp"),
    };
    progress.ensure_running()?;
    let size = bytes.len() as u64;
    if size >= original {
        return Ok(None);
    }
    let target = output(file, &format!("{}-min.{kind}", stem(file)), storage)?;
    if let Err(error) = fs::write(&target, &bytes) {
        storage.discard(&target);
        return Err(error.into());
    }
    Ok(Some((target, original, size)))
}

fn jpeg(image: &DynamicImage) -> Result<Vec<u8>> {
    let pixels = super::convert::flatten(image);
    let (width, height) = pixels.dimensions();
    catch_unwind(AssertUnwindSafe(|| -> std::io::Result<Vec<u8>> {
        let mut encoder = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
        encoder.set_size(width as usize, height as usize);
        encoder.set_quality(QUALITY);
        encoder.set_progressive_mode();
        encoder.set_optimize_scans(true);
        let mut started = encoder.start_compress(Vec::new())?;
        started.write_scanlines(pixels.as_raw())?;
        started.finish()
    }))
    .map_err(|_| anyhow!(tr().image_read_failed))?
    .map_err(Into::into)
}

fn webp(image: &DynamicImage) -> Result<Vec<u8>> {
    let pixels = image.to_rgba8();
    let opaque = pixels.pixels().all(|pixel| pixel.0[3] == u8::MAX);
    let memory = if opaque {
        let rgb = DynamicImage::ImageRgba8(pixels).into_rgb8();
        webp::Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height()).encode_simple(false, QUALITY)
    } else {
        webp::Encoder::from_rgba(pixels.as_raw(), pixels.width(), pixels.height()).encode_simple(false, QUALITY)
    };
    Ok(memory.map_err(|error| anyhow!("WebP: {error:?}"))?.to_vec())
}
