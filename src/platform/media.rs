use crate::i18n::{fill, tr};
use anyhow::{Result, anyhow};
use image::RgbaImage;
use std::fs;
use std::path::{Path, PathBuf};
use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::HSTRING;

const PDF_PAGES: u32 = 100;
const PDF_SCALE: f32 = 200. / 96.;
const PDF_SIDE: f32 = 4000.;
const FONTS: [&str; 4] = ["arial.ttf", "segoeui.ttf", "tahoma.ttf", "verdana.ttf"];

fn multithreaded<T: Send>(job: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                let result = job();
                if initialized {
                    unsafe { CoUninitialize() };
                }
                result
            })
            .join()
            .unwrap_or_else(|_| Err(anyhow!(tr().internal_error)))
    })
}

pub fn ocr_languages() -> Vec<(String, String)> {
    multithreaded(|| {
        let languages = OcrEngine::AvailableRecognizerLanguages()?;
        let mut found = Vec::new();
        for index in 0..languages.Size()? {
            let language = languages.GetAt(index)?;
            let name = language.NativeName()?.to_string_lossy();
            let mut letters = name.chars();
            let name = letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default();
            found.push((language.LanguageTag()?.to_string_lossy(), name));
        }
        Ok(found)
    })
    .unwrap_or_default()
}

pub fn recognize(image: &RgbaImage, language: &str) -> Result<String> {
    multithreaded(|| {
        let engine = if language.is_empty() {
            OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| anyhow!(tr().ocr_unavailable))?
        } else {
            Language::CreateLanguage(&HSTRING::from(language))
                .and_then(|wanted| OcrEngine::TryCreateFromLanguage(&wanted))
                .map_err(|_| anyhow!(fill(tr().ocr_language_missing, language)))?
        };
        let mut pixels = image.as_raw().clone();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&pixels)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &writer.DetachBuffer()?,
            BitmapPixelFormat::Bgra8,
            image.width() as i32,
            image.height() as i32,
        )?;
        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        let lines = result.Lines()?;
        let mut text = Vec::new();
        for index in 0..lines.Size()? {
            text.push(lines.GetAt(index)?.Text()?.to_string_lossy());
        }
        Ok(text.join("\r\n"))
    })
}

fn read_stream(stream: &InMemoryRandomAccessStream) -> Result<Vec<u8>> {
    let size = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    reader.LoadAsync(size)?.join()?;
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}

pub fn render_pdf(path: &Path, each: &mut (dyn FnMut(RgbaImage) -> Result<()> + Send)) -> Result<()> {
    let bytes = fs::read(path)?;
    multithreaded(move || {
        let source = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&source)?;
        writer.WriteBytes(&bytes)?;
        writer.StoreAsync()?.join()?;
        writer.FlushAsync()?.join()?;
        writer.DetachStream()?;
        source.Seek(0)?;
        let document = PdfDocument::LoadFromStreamAsync(&source)?.join().map_err(|_| anyhow!(tr().pdf_unreadable))?;
        for index in 0..document.PageCount()?.min(PDF_PAGES) {
            let page = document.GetPage(index)?;
            let size = page.Size()?;
            let scale = (PDF_SIDE / size.Width.max(size.Height).max(1.)).min(PDF_SCALE);
            let options = PdfPageRenderOptions::new()?;
            options.SetDestinationWidth((size.Width * scale).round() as u32)?;
            options.SetDestinationHeight((size.Height * scale).round() as u32)?;
            let rendered = InMemoryRandomAccessStream::new()?;
            page.RenderWithOptionsToStreamAsync(&rendered, &options)?.join()?;
            each(image::load_from_memory(&read_stream(&rendered)?)?.into_rgba8())?;
        }
        Ok(())
    })
}

pub fn font() -> Option<PathBuf> {
    let windows = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let folder = Path::new(&windows).join("Fonts");
    FONTS.iter().map(|name| folder.join(name)).find(|path| path.is_file())
}
