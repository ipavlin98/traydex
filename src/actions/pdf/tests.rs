use super::*;
use crate::actions::{Action, Kind, run};
use crate::store::Payload;
use crate::testing::{Fake, Sandbox, png};
use flate2::read::ZlibDecoder;
use image::codecs::jpeg::JpegEncoder;
use image::{ImageEncoder, Rgb, RgbImage, Rgba, RgbaImage};
use std::io::Read;

const ROTATE_90_EXIF: [u8; 26] = [
    0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

struct Bench {
    sandbox: Sandbox,
    fake: Fake,
    storage: Storage,
}

impl Bench {
    fn new() -> Self {
        let sandbox = Sandbox::new();
        let fake = Fake::new(&sandbox);
        let storage = sandbox.store().storage().clone();
        Self { sandbox, fake, storage }
    }

    fn pdf(&self, files: &[&PathBuf]) -> Result<Outcome> {
        let payload = Payload::Paths(files.iter().map(|file| (*file).clone()).collect());
        run(&Action::new(Kind::Pdf), payload, &Progress::default(), &self.storage, &self.fake)
    }

    fn jpeg(&self, name: &str, width: u32, height: u32, exif: Option<&[u8]>) -> PathBuf {
        let path = self.sandbox.join(name);
        let image = RgbImage::from_fn(width, height, |x, y| Rgb([(x * 40) as u8, (y * 40) as u8, 90]));
        let mut encoder = JpegEncoder::new(fs::File::create(&path).expect("file"));
        if let Some(exif) = exif {
            encoder.set_exif_metadata(exif.to_vec()).expect("exif");
        }
        encoder.write_image(image.as_raw(), width, height, image::ExtendedColorType::Rgb8).expect("jpeg");
        path
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn checked(path: &Path) -> Vec<u8> {
    let bytes = fs::read(path).expect("pdf");
    assert!(bytes.starts_with(b"%PDF-1.7\n"));
    assert!(bytes.ends_with(b"%%EOF\n"));
    let tail = String::from_utf8_lossy(&bytes[bytes.len() - 40..]).into_owned();
    let start: usize = tail
        .split("startxref\n")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .expect("startxref")
        .parse()
        .expect("offset");
    let table = String::from_utf8_lossy(&bytes[start..]).into_owned();
    let mut lines = table.lines();
    assert_eq!(lines.next(), Some("xref"));
    let size: usize = lines.next().and_then(|line| line.strip_prefix("0 ")).expect("size").parse().expect("count");
    assert_eq!(lines.next(), Some("0000000000 65535 f "));
    for id in 1..size {
        let offset: usize = lines.next().expect("entry")[..10].parse().expect("offset");
        assert!(bytes[offset..].starts_with(format!("{id} 0 obj\n").as_bytes()), "object {id} at {offset}");
    }
    bytes
}

fn inflated(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    let mut rest = bytes;
    while let Some(start) = find(rest, b">>\nstream\n") {
        let body = &rest[start + 10..];
        let end = find(body, b"\nendstream").expect("end");
        let mut data = Vec::new();
        if ZlibDecoder::new(&body[..end]).read_to_end(&mut data).is_ok() {
            found.push(data);
        }
        rest = &body[end..];
    }
    found
}

#[test]
fn images_join_in_natural_order_into_one_document() {
    let bench = Bench::new();
    let tenth = bench.sandbox.join("10.png");
    RgbaImage::from_pixel(33, 5, Rgba([1, 2, 3, 255])).save(&tenth).expect("png");
    let second = bench.sandbox.join("2.png");
    RgbaImage::from_pixel(22, 5, Rgba([1, 2, 3, 255])).save(&second).expect("png");
    let first = bench.jpeg("1.jpg", 11, 5, None);
    let outcome = bench.pdf(&[&tenth, &second, &first]).expect("pdf");
    let target = bench.sandbox.join(format!("{}.pdf", tr().document));
    assert_eq!(outcome.created, std::slice::from_ref(&target));
    assert_eq!(outcome.message, fill(tr().pdf_done, 3));
    let bytes = checked(&target);
    assert!(find(&bytes, b"/Count 3 ").is_some());
    let positions: Vec<usize> = ["/Width 11 ", "/Width 22 ", "/Width 33 "]
        .iter()
        .map(|width| find(&bytes, width.as_bytes()).expect(width))
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{positions:?}");
}

#[test]
fn jpeg_is_embedded_without_recompression() {
    let bench = Bench::new();
    let photo = bench.jpeg("фото отпуск.jpg", 40, 30, None);
    let outcome = bench.pdf(&[&photo]).expect("pdf");
    let target = bench.sandbox.join("фото отпуск.pdf");
    assert_eq!(outcome.created, std::slice::from_ref(&target));
    let bytes = checked(&target);
    assert!(find(&bytes, &fs::read(&photo).expect("jpeg")).is_some());
    assert!(find(&bytes, b"/DCTDecode").is_some());
    assert!(find(&bytes, b"/MediaBox [0 0 841.89 595.28]").is_some(), "landscape photo, landscape page");
}

#[test]
fn rotated_photos_stand_upright_and_transparency_is_kept() {
    let bench = Bench::new();
    let photo = bench.jpeg("rotated.jpg", 4, 2, Some(&ROTATE_90_EXIF));
    let clear = bench.sandbox.join("clear.png");
    RgbaImage::from_pixel(3, 3, Rgba([0, 0, 0, 0])).save(&clear).expect("png");
    let outcome = bench.pdf(&[&photo, &clear]).expect("pdf");
    let bytes = checked(&outcome.created[0]);
    assert!(find(&bytes, b"/Width 2 /Height 4 ").is_some());
    assert!(find(&bytes, b"/DCTDecode").is_none());
    assert!(find(&bytes, b"/SMask").is_some());
}

#[test]
fn text_files_become_paged_searchable_text() {
    let bench = Bench::new();
    bench.fake.state().font = crate::platform::font();
    assert!(bench.fake.font().is_some(), "Windows ships Arial or Segoe UI");
    let mut text = String::from("\u{feff}Привет, мир!\r\n");
    for line in 0..150 {
        text.push_str(&format!("Строка {line}\n"));
    }
    let note = bench.sandbox.file("заметка.txt", text);
    let picture = bench.sandbox.file("picture.png", png(2, 2));
    let outcome = bench.pdf(&[&note, &picture]).expect("pdf");
    let bytes = checked(&outcome.created[0]);
    assert!(find(&bytes, b"/Count 5 ").is_some(), "151 lines need 4 pages, plus the picture");
    assert!(find(&bytes, b"/Encoding /Identity-H").is_some());
    let streams = inflated(&bytes);
    let unicode = streams.iter().find(|data| find(data, b"beginbfchar").is_some()).expect("ToUnicode");
    for letter in ["041F", "0440", "0438", "0432", "0435", "0442"] {
        assert!(find(unicode, format!("<{letter}>").as_bytes()).is_some(), "{letter}");
    }
    assert!(find(unicode, b"<FEFF>").is_none());
}

#[test]
fn lines_wrap_within_the_page_width() {
    let data = fs::read(crate::platform::font().expect("system font")).expect("font");
    let mut font = Font::new(1, &data).expect("font");
    let width = 120.;
    let text = "Короткая строка\n\n\tотступ\nОченьдлинноесловобезпробеловкотороенепомещаетсявстроку и хвост";
    let lines = wrap(text, width, &mut font);
    for line in &lines {
        let span: f32 = line.chars().map(|c| font.glyph(c).1).sum();
        assert!(span <= width + 0.01, "{line:?} is {span}");
    }
    assert_eq!(lines[1], "");
    assert_eq!(lines[2], "    отступ");
    let letters = |text: &str| text.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    assert_eq!(letters(&lines.concat()), letters(text));
    assert!(lines.len() > 5);
}

#[test]
fn text_without_a_font_fails_cleanly() {
    let bench = Bench::new();
    let note = bench.sandbox.file("note.txt", "hello");
    let failure = bench.pdf(&[&note]).err().expect("no font");
    assert_eq!(failure.to_string(), tr().pdf_no_font);
    assert!(!bench.sandbox.join("note.pdf").exists());
}

#[test]
fn unsupported_files_are_skipped() {
    let bench = Bench::new();
    let archive = bench.sandbox.file("a.zip", "zip");
    let picture = bench.sandbox.file("b.png", png(2, 2));
    let outcome = bench.pdf(&[&archive, &picture]).expect("pdf");
    assert_eq!(outcome.message, format!("{} · {}", fill(tr().pdf_done, 1), fill(tr().skipped, 1)));
    assert_eq!(bench.pdf(&[&archive]).err().expect("nothing").to_string(), tr().pdf_needs);
}

#[test]
fn jpeg_component_count_comes_from_the_frame_header() {
    let frame = |marker: u8, components: u8| {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC4, 0x00, 0x03, 0x00];
        bytes.extend([0xFF, marker, 0x00, 0x0B, 0x08, 0x00, 0x10, 0x00, 0x10, components, 0x01, 0x11, 0x00]);
        bytes
    };
    assert_eq!(jpeg_components(&frame(0xC0, 3)), Some(3));
    assert_eq!(jpeg_components(&frame(0xC2, 4)), Some(4));
    assert_eq!(jpeg_components(&frame(0xC0, 1)), Some(1));
    assert_eq!(jpeg_components(b"not a jpeg at all"), None);
    assert_eq!(jpeg_components(&[0xFF, 0xD8]), None);
}
