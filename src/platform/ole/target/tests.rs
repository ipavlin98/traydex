use super::*;
use crate::platform::ole::set_reject;
use crate::platform::ole::source::Data;
use crate::store::Kind;
use crate::testing::{Sandbox, item, png};
use std::fs;
use windows::Win32::System::Memory::*;
use windows::core::{BOOL, HRESULT};

enum Contents {
    Stream(Vec<u8>),
    Global(Vec<u8>),
    Missing,
}

#[implement(IDataObject)]
struct Offer {
    formats: Vec<(u16, Vec<u8>)>,
    contents: Vec<Contents>,
}

fn medium(bytes: &[u8]) -> STGMEDIUM {
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1)).expect("alloc");
        let ptr = GlobalLock(handle);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        let _ = GlobalUnlock(handle);
        STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: handle },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        }
    }
}

impl IDataObject_Impl for Offer_Impl {
    fn GetData(&self, fmt: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let fmt = unsafe { &*fmt };
        if fmt.cfFormat == registered(w!("FileContents")) {
            return match self.contents.get(fmt.lindex as usize) {
                Some(Contents::Stream(bytes)) => Ok(STGMEDIUM {
                    tymed: TYMED_ISTREAM.0 as u32,
                    u: STGMEDIUM_0 { pstm: std::mem::ManuallyDrop::new(unsafe { SHCreateMemStream(Some(bytes)) }) },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                }),
                Some(Contents::Global(bytes)) => Ok(medium(bytes)),
                _ => Err(DV_E_FORMATETC.into()),
            };
        }
        self.formats
            .iter()
            .find(|(cf, _)| *cf == fmt.cfFormat)
            .map(|(_, bytes)| medium(bytes))
            .ok_or_else(|| DV_E_FORMATETC.into())
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, fmt: *const FORMATETC) -> HRESULT {
        let cf = unsafe { (*fmt).cfFormat };
        if self.formats.iter().any(|(offered, _)| *offered == cf) { S_OK } else { DV_E_FORMATETC }
    }

    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, _: *mut FORMATETC) -> HRESULT {
        E_NOTIMPL
    }

    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, _: u32) -> windows::core::Result<IEnumFORMATETC> {
        Err(E_NOTIMPL.into())
    }

    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<IAdviseSink>) -> windows::core::Result<u32> {
        Err(E_NOTIMPL.into())
    }

    fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(E_NOTIMPL.into())
    }
}

fn offer(formats: Vec<(u16, Vec<u8>)>, contents: Vec<Contents>) -> IDataObject {
    Offer { formats, contents }.into()
}

fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

struct Descriptor {
    name: &'static str,
    flags: u32,
    attributes: u32,
    size: u64,
}

fn descriptor(name: &'static str) -> Descriptor {
    Descriptor { name, flags: 0, attributes: 0, size: 0 }
}

fn group(descriptors: &[Descriptor]) -> Vec<u8> {
    let mut bytes = (descriptors.len() as u32).to_le_bytes().to_vec();
    for entry in descriptors {
        let mut block = vec![0u8; DESCRIPTOR_SIZE];
        block[0..4].copy_from_slice(&entry.flags.to_le_bytes());
        block[36..40].copy_from_slice(&entry.attributes.to_le_bytes());
        block[64..68].copy_from_slice(&((entry.size >> 32) as u32).to_le_bytes());
        block[68..72].copy_from_slice(&(entry.size as u32).to_le_bytes());
        let name = wide(entry.name);
        block[72..72 + name.len()].copy_from_slice(&name);
        bytes.extend(block);
    }
    bytes
}

fn group_format() -> u16 {
    registered(w!("FileGroupDescriptorW"))
}

fn text(payload: Payload) -> String {
    match payload {
        Payload::Text(text) => text,
        other => panic!("expected text, got {}", other.describe()),
    }
}

fn stored(payload: &Payload) -> Vec<PathBuf> {
    match payload {
        Payload::Stored(scratch) => scratch.paths().to_vec(),
        other => panic!("expected stored files, got {}", other.describe()),
    }
}

#[test]
fn little_endian_words_are_read_safely() {
    assert_eq!(u32_at(&[1, 0, 0, 0, 0xff], 0), 1);
    assert_eq!(u32_at(&[0, 1, 2, 3, 4], 1), 0x0403_0201);
    assert_eq!(u32_at(&[1, 2, 3], 0), 0);
    assert_eq!(u32_at(&[], 100), 0);
}

#[test]
fn text_decoders_stop_at_terminator() {
    assert_eq!(wide_string(&wide("Привет")), "Привет");
    assert_eq!(wide_string(&[b'a', 0, 0, 0, b'b', 0]), "a");
    assert_eq!(wide_string(&[b'a', 0, b'b']), "a");
    assert_eq!(ansi_string(b"plain\0garbage"), "plain");
    assert_eq!(ansi_string(b"\0"), "");
}

#[test]
fn dropped_files_win_over_everything_else() {
    let sandbox = Sandbox::new();
    let file = sandbox.file("a.txt", "x");
    let data: IDataObject = Data::new(vec![item(Kind::Text, "a", file.clone(), Some("x"))]).expect("data").into();
    match read_data(&data, &sandbox.storage()).expect("read") {
        Payload::Paths(paths) => assert_eq!(paths, [file]),
        other => panic!("expected paths, got {}", other.describe()),
    }
}

#[test]
fn virtual_files_are_saved_into_storage() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let mut sized = descriptor(r"Attachments\notes.txt");
    sized.flags = FD_FILESIZE;
    sized.size = 5;
    let data = offer(
        vec![(group_format(), group(&[descriptor("report.pdf"), sized]))],
        vec![Contents::Stream(b"%PDF-1.7 body".to_vec()), Contents::Global(b"hello world".to_vec())],
    );
    let payload = read_data(&data, &storage).expect("read");
    let paths = stored(&payload);
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|path| storage.owned(path)));
    assert_eq!(paths[0].file_name().and_then(|name| name.to_str()), Some("report.pdf"));
    assert_eq!(fs::read(&paths[0]).expect("pdf"), b"%PDF-1.7 body");
    assert_eq!(paths[1].file_name().and_then(|name| name.to_str()), Some("notes.txt"));
    assert_eq!(fs::read(&paths[1]).expect("notes"), b"hello");
    drop(payload);
    assert!(paths.iter().all(|path| !path.exists()));
}

#[test]
fn folders_shortcuts_and_unreadable_entries_are_skipped() {
    let sandbox = Sandbox::new();
    let mut folder = descriptor("Inbox");
    folder.flags = FD_ATTRIBUTES;
    folder.attributes = DIRECTORY;
    let mut readonly = descriptor("kept.txt");
    readonly.flags = FD_ATTRIBUTES;
    readonly.attributes = 0x1;
    let data = offer(
        vec![(
            group_format(),
            group(&[folder, descriptor("site.URL"), descriptor(""), descriptor("broken.bin"), readonly]),
        )],
        vec![
            Contents::Stream(Vec::new()),
            Contents::Stream(b"[InternetShortcut]".to_vec()),
            Contents::Stream(Vec::new()),
            Contents::Missing,
            Contents::Stream(b"kept".to_vec()),
        ],
    );
    let payload = read_data(&data, &sandbox.storage()).expect("read");
    let paths = stored(&payload);
    assert_eq!(paths.len(), 1);
    assert_eq!(fs::read(&paths[0]).expect("kept"), b"kept");
}

#[test]
fn descriptor_count_beyond_data_is_clamped() {
    let sandbox = Sandbox::new();
    let mut bytes = group(&[descriptor("a.txt")]);
    bytes[0..4].copy_from_slice(&1000u32.to_le_bytes());
    let data = offer(vec![(group_format(), bytes)], vec![Contents::Stream(b"a".to_vec())]);
    assert_eq!(stored(&read_data(&data, &sandbox.storage()).expect("read")).len(), 1);
    let tiny = offer(vec![(group_format(), vec![1, 0])], Vec::new());
    assert!(read_data(&tiny, &sandbox.storage()).is_err());
}

#[test]
fn virtual_file_names_cannot_escape_storage() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let data = offer(
        vec![(group_format(), group(&[descriptor(r"..\..\evil.txt"), descriptor(r"C:\Windows\x.dll")]))],
        vec![Contents::Stream(b"1".to_vec()), Contents::Stream(b"2".to_vec())],
    );
    let payload = read_data(&data, &storage).expect("read");
    let paths = stored(&payload);
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|path| storage.owned(path)));
    assert!(!sandbox.join("evil.txt").exists());
}

#[test]
fn images_prefer_png_then_dibv5_then_dib() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let all = offer(vec![(CF_DIB.0, vec![1; 40]), (CF_DIBV5.0, vec![2; 40]), (png_format(), png(1, 1))], Vec::new());
    assert!(
        matches!(read_data(&all, &storage).expect("png"), Payload::Image(ImageData::Png(bytes)) if bytes.starts_with(&png(1, 1)))
    );
    let dibs = offer(vec![(CF_DIB.0, vec![1; 40]), (CF_DIBV5.0, vec![2; 40])], Vec::new());
    assert!(matches!(read_data(&dibs, &storage).expect("dib"), Payload::Image(ImageData::Dib(bytes)) if bytes[0] == 2));
    let dib = offer(vec![(CF_DIB.0, vec![1; 40])], Vec::new());
    assert!(matches!(read_data(&dib, &storage).expect("dib"), Payload::Image(ImageData::Dib(bytes)) if bytes[0] == 1));
}

#[test]
fn text_prefers_url_then_unicode_then_ansi() {
    let sandbox = Sandbox::new();
    let storage = sandbox.storage();
    let url = registered(w!("UniformResourceLocatorW"));
    let all = offer(
        vec![(CF_TEXT.0, b"ansi\0".to_vec()), (CF_UNICODETEXT.0, wide("уникод")), (url, wide("https://x.example"))],
        Vec::new(),
    );
    assert_eq!(text(read_data(&all, &storage).expect("url")), "https://x.example");
    let both = offer(vec![(CF_TEXT.0, b"ansi\0".to_vec()), (CF_UNICODETEXT.0, wide("уникод"))], Vec::new());
    assert_eq!(text(read_data(&both, &storage).expect("unicode")), "уникод");
    let blank = offer(vec![(CF_TEXT.0, b"ansi text\0".to_vec()), (CF_UNICODETEXT.0, wide(" \r\n "))], Vec::new());
    assert_eq!(text(read_data(&blank, &storage).expect("ansi")), "ansi text");
}

#[test]
fn unknown_or_empty_data_is_unsupported() {
    let sandbox = Sandbox::new();
    let nothing = offer(Vec::new(), Vec::new());
    assert_eq!(read_data(&nothing, &sandbox.storage()).err().expect("error").to_string(), tr().unsupported);
    let blank = offer(vec![(CF_UNICODETEXT.0, wide("   "))], Vec::new());
    assert!(read_data(&blank, &sandbox.storage()).is_err());
    assert_eq!(fs::read_dir(sandbox.path()).expect("sandbox").count(), 0);
}

#[test]
fn offered_formats_are_named_for_logging() {
    let url = registered(w!("UniformResourceLocatorW"));
    let data = offer(vec![(CF_UNICODETEXT.0, wide("x")), (url, wide("y")), (png_format(), png(1, 1))], Vec::new());
    assert_eq!(offered(&data), ["PNG", "URL", "Unicode text"]);
    assert!(offered(&offer(Vec::new(), Vec::new())).is_empty());
}

#[test]
fn drop_effect_prefers_copy_and_honours_rejection() {
    assert_eq!(choose(true, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK), DROPEFFECT_COPY);
    assert_eq!(choose(true, DROPEFFECT_LINK), DROPEFFECT_LINK);
    assert_eq!(choose(true, DROPEFFECT_MOVE), DROPEFFECT_NONE);
    assert_eq!(choose(false, DROPEFFECT_COPY), DROPEFFECT_NONE);
    set_reject(true);
    let rejected = choose(true, DROPEFFECT_COPY);
    set_reject(false);
    assert_eq!(rejected, DROPEFFECT_NONE);
}

#[test]
fn effect_pointers_may_be_null() {
    assert_eq!(allowed(std::ptr::null_mut()), DROPEFFECT_NONE);
    set_effect(std::ptr::null_mut(), DROPEFFECT_COPY);
    let mut effect = DROPEFFECT_COPY;
    assert!(refuse(&mut effect).is_err());
    assert_eq!(effect, DROPEFFECT_NONE);
}
