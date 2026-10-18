use super::{Outcome, Progress, by_name, extension, output, stem};
use crate::i18n::{fill, tr};
use crate::store::{self, Storage};
use crate::system::Services;
use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{ColorType, DynamicImage, ImageDecoder, ImageReader};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use ttf_parser::{Face, GlyphId, name_id};

const A4: (f32, f32) = (595.28, 841.89);
const IMAGE_MARGIN: f32 = 28.35;
const TEXT_MARGIN: f32 = 56.69;
const FONT_SIZE: f32 = 11.;
const LEADING: f32 = 15.4;
const POINTS_PER_PIXEL: f32 = 0.75;
const IMAGE_SIDE: u32 = 3508;
const TEXT_LIMIT: u64 = 8 * 1024 * 1024;
const TAB: &str = "    ";
const TEXTS: [&str; 9] = ["txt", "md", "log", "csv", "json", "xml", "ini", "yaml", "yml"];

fn is_text(path: &Path) -> bool {
    TEXTS.contains(&extension(path).as_str())
}

pub(super) fn printable(path: &Path) -> bool {
    store::is_image(path) || is_text(path)
}

pub(super) fn pdf(files: &[PathBuf], progress: &Progress, storage: &Storage, system: &dyn Services) -> Result<Outcome> {
    let mut sources: Vec<PathBuf> = files.iter().filter(|file| file.is_file() && printable(file)).cloned().collect();
    if sources.is_empty() {
        bail!(tr().pdf_needs);
    }
    by_name(&mut sources);
    let data = match sources.iter().any(|file| is_text(file)) {
        true => Some(fs::read(system.font().context(tr().pdf_no_font)?)?),
        false => None,
    };
    let mut writer = Writer::new();
    let catalog = writer.reserve();
    let tree = writer.reserve();
    let mut font = data.as_deref().map(|data| Font::new(writer.reserve(), data)).transpose()?;
    let mut pages = Vec::new();
    for (done, file) in sources.iter().enumerate() {
        if !progress.step(done, sources.len()) {
            break;
        }
        let added = match font.as_mut().filter(|_| is_text(file)) {
            Some(font) => text_pages(&mut writer, file, font, tree),
            None => image_page(&mut writer, file, tree).map(|page| vec![page]),
        };
        pages.extend(added.with_context(|| super::base(file))?);
    }
    if let Some(font) = font {
        font.write(&mut writer);
    }
    let kids: Vec<String> = pages.iter().map(|page| format!("{page} 0 R")).collect();
    writer.object(tree, &format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()));
    writer.object(catalog, &format!("<< /Type /Catalog /Pages {tree} 0 R >>"));
    let name = match sources.as_slice() {
        [file] => stem(file),
        _ => tr().document.to_string(),
    };
    let target = output(sources.first().context(tr().pdf_needs)?, &format!("{name}.pdf"), storage)?;
    if let Err(error) = fs::write(&target, writer.finish(catalog)) {
        storage.discard(&target);
        return Err(error.into());
    }
    let mut message = fill(tr().pdf_done, pages.len());
    let skipped = files.len() - sources.len();
    if skipped > 0 {
        message = format!("{message} · {}", fill(tr().skipped, skipped));
    }
    Ok(Outcome { message, created: vec![target], clipboard: None, show: None })
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

struct Writer {
    data: Vec<u8>,
    offsets: Vec<usize>,
}

impl Writer {
    fn new() -> Self {
        Self { data: b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec(), offsets: Vec::new() }
    }

    fn reserve(&mut self) -> usize {
        self.offsets.push(0);
        self.offsets.len()
    }

    fn mark(&mut self, id: usize) {
        if let Some(offset) = id.checked_sub(1).and_then(|index| self.offsets.get_mut(index)) {
            *offset = self.data.len();
        }
    }

    fn object(&mut self, id: usize, body: &str) {
        self.mark(id);
        self.data.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    fn stream(&mut self, id: usize, entries: &str, bytes: &[u8]) {
        self.mark(id);
        self.data.extend_from_slice(format!("{id} 0 obj\n<<{entries} /Length {} >>\nstream\n", bytes.len()).as_bytes());
        self.data.extend_from_slice(bytes);
        self.data.extend_from_slice(b"\nendstream\nendobj\n");
    }

    fn finish(mut self, root: usize) -> Vec<u8> {
        let start = self.data.len();
        let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", self.offsets.len() + 1);
        for offset in &self.offsets {
            table.push_str(&format!("{offset:010} 00000 n \n"));
        }
        table.push_str(&format!(
            "trailer\n<< /Size {} /Root {root} 0 R >>\nstartxref\n{start}\n%%EOF\n",
            self.offsets.len() + 1
        ));
        self.data.extend_from_slice(table.as_bytes());
        self.data
    }
}

fn page(writer: &mut Writer, parent: usize, size: (f32, f32), resources: &str, content: &str) -> usize {
    let stream = writer.reserve();
    writer.stream(stream, " /Filter /FlateDecode", &deflate(content.as_bytes()));
    let page = writer.reserve();
    writer.object(
        page,
        &format!(
            "<< /Type /Page /Parent {parent} 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << {resources} >> /Contents {stream} 0 R >>",
            size.0, size.1
        ),
    );
    page
}

fn image_page(writer: &mut Writer, file: &Path, parent: usize) -> Result<usize> {
    let (image, width, height) = embed_image(writer, file).context(tr().image_read_failed)?;
    let size = if width > height { (A4.1, A4.0) } else { A4 };
    let natural = (width as f32 * POINTS_PER_PIXEL, height as f32 * POINTS_PER_PIXEL);
    let scale = ((size.0 - 2. * IMAGE_MARGIN) / natural.0).min((size.1 - 2. * IMAGE_MARGIN) / natural.1).min(1.);
    let (w, h) = (natural.0 * scale, natural.1 * scale);
    let (x, y) = ((size.0 - w) / 2., (size.1 - h) / 2.);
    let content = format!("q {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm /Im0 Do Q");
    Ok(page(writer, parent, size, &format!("/XObject << /Im0 {image} 0 R >>"), &content))
}

fn jpeg_components(bytes: &[u8]) -> Option<u8> {
    let mut at = 2;
    loop {
        let &[0xFF, marker, high, low] = bytes.get(at..at + 4)? else {
            return None;
        };
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return bytes.get(at + 9).copied();
        }
        at += 2 + usize::from(u16::from_be_bytes([high, low]));
    }
}

fn embed_image(writer: &mut Writer, file: &Path) -> Result<(usize, u32, u32)> {
    let mut decoder = ImageReader::open(file)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let (width, height) = decoder.dimensions();
    if matches!(extension(file).as_str(), "jpg" | "jpeg") && orientation == Orientation::NoTransforms {
        let bytes = fs::read(file)?;
        let space = match (decoder.color_type(), jpeg_components(&bytes)) {
            (ColorType::L8, Some(1)) => Some("/DeviceGray"),
            (ColorType::Rgb8, Some(3)) => Some("/DeviceRGB"),
            _ => None,
        };
        if let Some(space) = space {
            let id = writer.reserve();
            let entries = format!(
                " /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace {space} /BitsPerComponent 8 /Filter /DCTDecode"
            );
            writer.stream(id, &entries, &bytes);
            return Ok((id, width, height));
        }
    }
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    if image.width().max(image.height()) > IMAGE_SIDE {
        image = image.resize(IMAGE_SIDE, IMAGE_SIDE, FilterType::Lanczos3);
    }
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let mask = if rgba.pixels().all(|pixel| pixel.0[3] == u8::MAX) {
        String::new()
    } else {
        let alpha: Vec<u8> = rgba.pixels().map(|pixel| pixel.0[3]).collect();
        let id = writer.reserve();
        let entries = format!(
            " /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode"
        );
        writer.stream(id, &entries, &deflate(&alpha));
        format!(" /SMask {id} 0 R")
    };
    let rgb: Vec<u8> = rgba.pixels().flat_map(|pixel| [pixel.0[0], pixel.0[1], pixel.0[2]]).collect();
    let id = writer.reserve();
    let entries = format!(
        " /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode{mask}"
    );
    writer.stream(id, &entries, &deflate(&rgb));
    Ok((id, width, height))
}

fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        let words: Vec<u16> =
            rest.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        return String::from_utf16_lossy(&words);
    }
    String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned()
}

fn text_pages(writer: &mut Writer, file: &Path, font: &mut Font, parent: usize) -> Result<Vec<usize>> {
    if fs::metadata(file)?.len() > TEXT_LIMIT {
        bail!(tr().too_large);
    }
    let text = decode_text(&fs::read(file)?);
    let lines = wrap(&text, A4.0 - 2. * TEXT_MARGIN, font);
    let rows = ((A4.1 - 2. * TEXT_MARGIN) / LEADING).floor() as usize;
    let chunks: Vec<&[String]> = if lines.is_empty() { vec![&[]] } else { lines.chunks(rows).collect() };
    let resources = format!("/Font << /F1 {} 0 R >>", font.id);
    Ok(chunks
        .into_iter()
        .map(|chunk| {
            let mut content = format!(
                "BT /F1 {FONT_SIZE} Tf {LEADING} TL {TEXT_MARGIN:.2} {:.2} Td\n",
                A4.1 - TEXT_MARGIN - FONT_SIZE
            );
            for line in chunk {
                let glyphs: String = line.chars().map(|c| format!("{:04X}", font.glyph(c).0)).collect();
                content.push_str(&format!("<{glyphs}> Tj T*\n"));
            }
            content.push_str("ET");
            page(writer, parent, A4, &resources, &content)
        })
        .collect())
}

fn wrap(text: &str, width: f32, font: &mut Font) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let paragraph: String = paragraph.replace('\t', TAB).chars().filter(|c| !c.is_control()).collect();
        let mut line = String::new();
        let mut used = 0.;
        for word in paragraph.split_inclusive(' ') {
            let span: f32 = word.chars().map(|c| font.glyph(c).1).sum();
            if used + span > width && !line.is_empty() {
                lines.push(line.trim_end().to_string());
                line.clear();
                used = 0.;
            }
            if span <= width {
                line.push_str(word);
                used += span;
                continue;
            }
            for c in word.chars() {
                let advance = font.glyph(c).1;
                if used + advance > width && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    used = 0.;
                }
                line.push(c);
                used += advance;
            }
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}

struct Font<'a> {
    id: usize,
    data: &'a [u8],
    face: Face<'a>,
    used: BTreeMap<u16, char>,
}

impl<'a> Font<'a> {
    fn new(id: usize, data: &'a [u8]) -> Result<Self> {
        let face = Face::parse(data, 0).ok().filter(|face| face.tables().glyf.is_some()).context(tr().pdf_no_font)?;
        Ok(Self { id, data, face, used: BTreeMap::new() })
    }

    fn units(&self, value: impl Into<f32>) -> f32 {
        value.into() * 1000. / self.face.units_per_em() as f32
    }

    fn glyph(&mut self, c: char) -> (u16, f32) {
        let id = self.face.glyph_index(c).unwrap_or(GlyphId(0)).0;
        self.used.entry(id).or_insert(c);
        let advance = self.face.glyph_hor_advance(GlyphId(id)).unwrap_or(0);
        (id, self.units(advance) * FONT_SIZE / 1000.)
    }

    fn name(&self) -> String {
        let name = self
            .face
            .names()
            .into_iter()
            .find(|name| name.name_id == name_id::POST_SCRIPT_NAME && name.is_unicode())
            .and_then(|name| name.to_string())
            .unwrap_or_default();
        let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        if name.is_empty() { "Font".into() } else { name }
    }

    fn write(self, writer: &mut Writer) {
        let name = self.name();
        let (cid, descriptor, file, unicode) = (writer.reserve(), writer.reserve(), writer.reserve(), writer.reserve());
        writer.stream(file, &format!(" /Length1 {} /Filter /FlateDecode", self.data.len()), &deflate(self.data));
        let bbox = self.face.global_bounding_box();
        let ascent = self.units(self.face.ascender());
        let cap = self.units(self.face.capital_height().unwrap_or(self.face.ascender()));
        writer.object(
            descriptor,
            &format!(
                "<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [{:.0} {:.0} {:.0} {:.0}] /ItalicAngle 0 /Ascent {ascent:.0} /Descent {:.0} /CapHeight {cap:.0} /StemV 80 /FontFile2 {file} 0 R >>",
                self.units(bbox.x_min),
                self.units(bbox.y_min),
                self.units(bbox.x_max),
                self.units(bbox.y_max),
                self.units(self.face.descender()),
            ),
        );
        let widths: Vec<String> = self
            .used
            .keys()
            .map(|&id| format!("{id} [{:.0}]", self.units(self.face.glyph_hor_advance(GlyphId(id)).unwrap_or(0))))
            .collect();
        writer.object(
            cid,
            &format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /W [{}] /CIDToGIDMap /Identity >>",
                widths.join(" ")
            ),
        );
        writer.stream(unicode, " /Filter /FlateDecode", &deflate(self.cmap().as_bytes()));
        writer.object(
            self.id,
            &format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {unicode} 0 R >>"
            ),
        );
    }

    fn cmap(&self) -> String {
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        );
        let pairs: Vec<(&u16, &char)> = self.used.iter().filter(|(id, _)| **id != 0).collect();
        for block in pairs.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", block.len()));
            for (id, c) in block {
                let text: String = c.encode_utf16(&mut [0; 2]).iter().map(|unit| format!("{unit:04X}")).collect();
                cmap.push_str(&format!("<{id:04X}> <{text}>\n"));
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        cmap
    }
}

#[cfg(test)]
mod tests;
