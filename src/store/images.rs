use super::{ImageData, Storage};
use crate::i18n::{fill, tr};
use anyhow::{Context, Result};
use image::ImageDecoder;
use std::fs;
use std::path::{Path, PathBuf};

pub fn open_image(path: &Path) -> Result<image::DynamicImage> {
    Ok(image::ImageReader::open(path)?.with_guessed_format()?.decode()?)
}

pub fn open_oriented(path: &Path) -> Result<image::DynamicImage> {
    let mut decoder = image::ImageReader::open(path)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image)
}

pub fn decode(data: ImageData) -> Result<image::RgbaImage> {
    match data {
        ImageData::Png(bytes) => Ok(image::load_from_memory(&bytes).context(tr().image_read_failed)?.into_rgba8()),
        ImageData::Dib(bytes) => decode_dib(&bytes).context(tr().image_read_failed),
    }
}

pub fn title(image: &image::RgbaImage) -> String {
    fill(&fill(tr().image_title, image.width()), image.height())
}

pub fn write_png(storage: &Storage, image: &image::RgbaImage, path: &Path) -> Result<()> {
    if let Err(error) = image.save_with_format(path, image::ImageFormat::Png) {
        storage.discard(path);
        return Err(error.into());
    }
    Ok(())
}

pub fn save_image(storage: &Storage, data: ImageData) -> Result<(PathBuf, String)> {
    let image = decode(data)?;
    let path = storage.unique("image", "png");
    write_png(storage, &image, &path)?;
    Ok((path, title(&image)))
}

pub fn png_bytes(path: &Path) -> Result<Vec<u8>> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Ok(fs::read(path)?);
    }
    let mut png = std::io::Cursor::new(Vec::new());
    open_image(path)?.to_rgba8().write_to(&mut png, image::ImageFormat::Png)?;
    Ok(png.into_inner())
}

pub fn dib_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut bmp = std::io::Cursor::new(Vec::new());
    open_image(path)?.to_rgba8().write_to(&mut bmp, image::ImageFormat::Bmp)?;
    let bmp = bmp.into_inner();
    Ok(bmp.get(14..).context(tr().image_read_failed)?.to_vec())
}

fn decode_dib(dib: &[u8]) -> Option<image::RgbaImage> {
    if dib.len() < 40 {
        return None;
    }
    let header = u32::from_le_bytes(dib[..4].try_into().ok()?) as usize;
    if header < 40 || header > dib.len() {
        return None;
    }
    let bpp = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    let used = u32::from_le_bytes(dib[32..36].try_into().ok()?) as usize;
    let colors = if used > 0 {
        used
    } else if bpp <= 8 {
        1usize << bpp
    } else {
        0
    };
    let masks = match (header, compression) {
        (40, 3) => 12,
        (40, 6) => 16,
        _ => 0,
    };
    let offset = 14usize.checked_add(header)?.checked_add(colors.checked_mul(4)?)?.checked_add(masks)?;
    if offset > dib.len() + 14 {
        return None;
    }
    let mut bmp = Vec::with_capacity(dib.len() + 14);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((dib.len() + 14) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&(offset as u32).to_le_bytes());
    bmp.extend_from_slice(dib);
    image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok().map(|i| i.into_rgba8())
}
