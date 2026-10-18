use super::{Outcome, Progress, Report, extension, output, stem};
use crate::i18n::{fill, tr};
use crate::store::{self, Storage};
use anyhow::{Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use std::path::{Path, PathBuf};
use std::{fs, io};

const JPEG_QUALITY: u8 = 90;

pub(super) fn convertible(path: &Path, format: ImageFormat) -> bool {
    store::is_image(path) && !format.extensions_str().contains(&extension(path).as_str())
}

pub(super) fn convert(
    files: &[PathBuf],
    format: ImageFormat,
    progress: &Progress,
    storage: &Storage,
) -> Result<Outcome> {
    let extensions = format.extensions_str();
    let name = extensions.first().copied().unwrap_or("png");
    let mut report = Report::default();
    let mut same = 0;
    for (done, file) in files.iter().enumerate() {
        if !progress.step(done, files.len()) {
            break;
        }
        if !store::is_image(file) {
            report.skipped += 1;
        } else if extensions.contains(&extension(file).as_str()) {
            report.skipped += 1;
            same += 1;
        } else {
            match convert_one(file, format, name, progress, storage) {
                Ok(target) => report.created.push(target),
                Err(error) => report.fail(file, error),
            }
        }
    }
    let wrong = if same == files.len() {
        fill(tr().already_format, name.to_uppercase())
    } else {
        tr().needs_images.to_string()
    };
    report.finish(wrong)
}

fn convert_one(
    file: &Path,
    format: ImageFormat,
    name: &str,
    progress: &Progress,
    storage: &Storage,
) -> Result<PathBuf> {
    let image = store::open_image(file).context(tr().image_read_failed)?;
    progress.ensure_running()?;
    let target = output(file, &format!("{}.{name}", stem(file)), storage)?;
    let written = fs::File::create(&target).map_err(anyhow::Error::from).and_then(|file| {
        let mut writer = progress.guard(io::BufWriter::new(file));
        if format == ImageFormat::Jpeg {
            DynamicImage::ImageRgb8(flatten(&image))
                .write_with_encoder(JpegEncoder::new_with_quality(&mut writer, JPEG_QUALITY))
                .map_err(Into::into)
        } else {
            image.write_to(&mut writer, format).map_err(Into::into)
        }
    });
    if let Err(error) = written {
        storage.discard(&target);
        return Err(error);
    }
    Ok(target)
}

pub(super) fn flatten(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let blend = |c: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32)) / 255) as u8;
        Rgb([blend(r), blend(g), blend(b)])
    })
}
