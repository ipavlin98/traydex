use super::*;
use crate::platform::ole::target::read_data;
use crate::store::Payload;
use crate::store::Storage;
use crate::testing::{Sandbox, item, png};
use std::path::PathBuf;

fn object(items: Vec<Item>) -> IDataObject {
    Data::new(items).expect("data").into()
}

fn query(data: &IDataObject, cf: u16) -> HRESULT {
    unsafe { data.QueryGetData(&format(cf, TYMED_HGLOBAL.0 as u32)) }
}

fn get(data: &IDataObject, cf: u16) -> Option<Vec<u8>> {
    unsafe {
        let mut medium = data.GetData(&format(cf, TYMED_HGLOBAL.0 as u32)).ok()?;
        let bytes = global_bytes(medium.u.hGlobal);
        ReleaseStgMedium(&mut medium);
        bytes
    }
}

fn listed(data: &IDataObject) -> Vec<u16> {
    let formats = unsafe { data.EnumFormatEtc(DATADIR_GET.0 as u32) }.expect("enum");
    let mut found = Vec::new();
    loop {
        let mut next = [FORMATETC::default()];
        let mut fetched = 0u32;
        if unsafe { formats.Next(&mut next, Some(&mut fetched as *mut u32)) } != S_OK || fetched == 0 {
            return found;
        }
        found.push(next[0].cfFormat);
    }
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

#[test]
fn empty_selection_cannot_be_exported() {
    assert_eq!(Data::new(Vec::new()).err().expect("empty").to_string(), tr().empty_contents);
}

#[test]
fn missing_original_file_blocks_export() {
    let sandbox = Sandbox::new();
    let gone = item(Kind::File, "report.pdf", sandbox.join("report.pdf"), None);
    assert_eq!(Data::new(vec![gone]).err().expect("missing").to_string(), fill(tr().missing_original, "report.pdf"));
}

#[test]
fn missing_note_file_is_restored_from_text() {
    let sandbox = Sandbox::new();
    let path = sandbox.join("root/content/note-1.txt");
    let note = item(Kind::Text, "note", path.clone(), Some("restored text"));
    Data::new(vec![note]).expect("data");
    assert_eq!(std::fs::read_to_string(path).expect("note"), "restored text");
}

#[test]
fn files_are_offered_as_drop_list() {
    let sandbox = Sandbox::new();
    let a = sandbox.file("a b.txt", "x");
    let b = sandbox.folder("Папка");
    let data = object(vec![item(Kind::File, "a", a.clone(), None), item(Kind::Folder, "b", b.clone(), None)]);
    assert_eq!(listed(&data), [CF_HDROP.0]);
    assert_eq!(query(&data, CF_HDROP.0), S_OK);
    assert_eq!(query(&data, CF_UNICODETEXT.0), DV_E_FORMATETC);
    assert!(get(&data, CF_UNICODETEXT.0).is_none());
    match read_data(&data, &Storage::default()).expect("read") {
        Payload::Paths(paths) => assert_eq!(paths, [a, b]),
        _ => panic!("expected paths"),
    }
}

#[test]
fn notes_are_offered_as_files_and_text() {
    let sandbox = Sandbox::new();
    let first = item(Kind::Text, "1", sandbox.file("1.txt", "first"), Some("first"));
    let second = item(Kind::Link, "2", sandbox.file("2.txt", "https://x.example"), Some("https://x.example"));
    let data = object(vec![first, second]);
    assert_eq!(listed(&data), [CF_HDROP.0, CF_UNICODETEXT.0]);
    let text = get(&data, CF_UNICODETEXT.0).expect("text");
    assert!(text.starts_with(&utf16("first\r\nhttps://x.example")));
}

#[test]
fn single_image_is_offered_as_bitmap_too() {
    let sandbox = Sandbox::new();
    let image = sandbox.file("pic.png", png(3, 2));
    let data = object(vec![item(Kind::Image, "pic", image.clone(), None)]);
    let png_cf = png_format();
    assert_eq!(listed(&data), [CF_HDROP.0, png_cf, CF_DIB.0]);
    assert!(get(&data, png_cf).expect("png").starts_with(&png(3, 2)));
    let dib = get(&data, CF_DIB.0).expect("dib");
    let decoded = store::decode(store::ImageData::Dib(dib)).expect("decode");
    assert_eq!(decoded.dimensions(), (3, 2));
    let two = object(vec![item(Kind::Image, "a", image.clone(), None), item(Kind::Image, "b", image, None)]);
    assert_eq!(listed(&two), [CF_HDROP.0]);
}

#[test]
fn unreadable_image_formats_fail_softly() {
    let sandbox = Sandbox::new();
    let broken = sandbox.file("broken.jpg", "not an image");
    let data = object(vec![item(Kind::Image, "b", broken, None)]);
    assert_eq!(query(&data, CF_DIB.0), S_OK);
    assert!(get(&data, CF_DIB.0).is_none());
    assert!(get(&data, CF_HDROP.0).is_some());
}

#[test]
fn queries_validate_aspect_medium_and_pointer() {
    let sandbox = Sandbox::new();
    let data = object(vec![item(Kind::File, "a", sandbox.file("a.txt", "x"), None)]);
    let mut wrong_aspect = format(CF_HDROP.0, TYMED_HGLOBAL.0 as u32);
    wrong_aspect.dwAspect = DVASPECT_ICON.0;
    assert_eq!(unsafe { data.QueryGetData(&wrong_aspect) }, DV_E_DVASPECT);
    assert_eq!(unsafe { data.QueryGetData(&format(CF_HDROP.0, TYMED_ISTREAM.0 as u32)) }, DV_E_TYMED);
    assert_eq!(unsafe { data.QueryGetData(std::ptr::null()) }, E_INVALIDARG);
    assert!(unsafe { data.GetData(&format(CF_TEXT.0, TYMED_HGLOBAL.0 as u32)) }.is_err());
    assert!(unsafe { data.EnumFormatEtc(DATADIR_SET.0 as u32) }.is_err());
}

#[test]
fn shell_can_attach_its_own_formats() {
    let sandbox = Sandbox::new();
    let data = object(vec![item(Kind::File, "a", sandbox.file("a.txt", "x"), None)]);
    let custom = crate::platform::ole::registered(windows::core::w!("Traydex.Test.Format"));
    let set = |bytes: &[u8]| unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).expect("alloc");
        let ptr = GlobalLock(handle);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        let _ = GlobalUnlock(handle);
        let medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: handle },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        };
        data.SetData(&format(custom, TYMED_HGLOBAL.0 as u32), &medium, true).expect("set");
    };
    set(b"first");
    set(b"second");
    assert_eq!(query(&data, custom), S_OK);
    assert!(get(&data, custom).expect("custom").starts_with(b"second"));
    assert_eq!(listed(&data), [CF_HDROP.0, custom]);
    let stream = STGMEDIUM {
        tymed: TYMED_ISTREAM.0 as u32,
        u: STGMEDIUM_0 { hGlobal: HGLOBAL::default() },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    };
    assert!(unsafe { data.SetData(&format(custom, TYMED_ISTREAM.0 as u32), &stream, false) }.is_err());
}

#[test]
fn drop_list_layout_is_wide_and_double_terminated() {
    let items = [
        item(Kind::File, "a", PathBuf::from(r"C:\a.txt"), None),
        item(Kind::File, "b", PathBuf::from(r"D:\Ж.txt"), None),
    ];
    let bytes = hdrop(&items);
    assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().expect("offset")), 20);
    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().expect("wide")), 1);
    let words: Vec<u16> =
        bytes[20..].as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
    let expected: Vec<u16> =
        r"C:\a.txt".encode_utf16().chain([0]).chain(r"D:\Ж.txt".encode_utf16()).chain([0, 0]).collect();
    assert_eq!(words, expected);
}
