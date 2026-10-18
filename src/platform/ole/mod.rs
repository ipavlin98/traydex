mod source;
mod target;

pub use source::drag;

use self::source::Data;
use self::target::{Target, read_data};
use super::Payload;
use crate::i18n::tr;
use crate::os::ansi_string;
use crate::store::{Item, Storage};
use anyhow::Context;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::*;
use windows::Win32::UI::Shell::*;
use windows::core::{PCWSTR, w};

const MAX_BYTES: usize = 256 * 1024 * 1024;

static EXPORTING: AtomicBool = AtomicBool::new(false);
static REJECT: AtomicBool = AtomicBool::new(false);

thread_local! {
    static COPIED: Cell<bool> = const { Cell::new(false) };
}

pub fn exporting() -> bool {
    EXPORTING.load(Ordering::Relaxed)
}

pub fn set_reject(reject: bool) {
    REJECT.store(reject, Ordering::Relaxed);
}

fn rejecting() -> bool {
    REJECT.load(Ordering::Relaxed)
}

fn registered(name: PCWSTR) -> u16 {
    unsafe { RegisterClipboardFormatW(name) as u16 }
}

fn png_format() -> u16 {
    registered(w!("PNG"))
}

fn format(cf: u16, tymed: u32) -> FORMATETC {
    FORMATETC { cfFormat: cf, dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed, ..Default::default() }
}

pub(super) fn register(hwnd: HWND, storage: Storage) -> anyhow::Result<()> {
    unsafe {
        let _ = RevokeDragDrop(hwnd);
        let helper = CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER).ok();
        let target: IDropTarget =
            Target { hwnd, helper, accepted: Cell::new(false), pointer: Cell::new(None), storage }.into();
        RegisterDragDrop(hwnd, &target).context(tr().register_failed)?;
    }
    Ok(())
}

unsafe fn global_bytes(handle: HGLOBAL) -> Option<Vec<u8>> {
    unsafe {
        let length = GlobalSize(handle);
        if length > MAX_BYTES {
            log::warn!("Ignored {length} bytes of dropped data: over the limit");
            return None;
        }
        if length == 0 {
            return None;
        }
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr as *const u8, length).to_vec();
        let _ = GlobalUnlock(handle);
        Some(bytes)
    }
}

fn bytes(data: &IDataObject, cf: u16) -> Option<Vec<u8>> {
    unsafe {
        let mut medium = data.GetData(&format(cf, TYMED_HGLOBAL.0 as u32)).ok()?;
        let result = if medium.tymed == TYMED_HGLOBAL.0 as u32 { global_bytes(medium.u.hGlobal) } else { None };
        ReleaseStgMedium(&mut medium);
        result
    }
}

fn wide_string(bytes: &[u8]) -> String {
    let words: Vec<u16> =
        bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|c| *c != 0).collect();
    String::from_utf16_lossy(&words)
}

pub fn copy(items: Vec<Item>) -> anyhow::Result<()> {
    let data: IDataObject = Data::new(items)?.into();
    unsafe { OleSetClipboard(&data)? };
    COPIED.set(true);
    Ok(())
}

pub fn flush_clipboard() {
    if COPIED.get() {
        unsafe {
            let _ = OleFlushClipboard();
        }
    }
}

pub fn paste(storage: &Storage) -> anyhow::Result<Payload> {
    read_data(&unsafe { OleGetClipboard() }?, storage)
}
