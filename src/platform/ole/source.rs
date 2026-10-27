use super::EXPORTING;
use super::{format, global_bytes, png_format};
use crate::i18n::{fill, tr};
use crate::platform::drag::{class_name, process_name};
use crate::platform::window::panel;
use crate::platform::{Event, guarded, send};
use crate::store::{self, Item, Kind};
use anyhow::bail;
use std::cell::{Cell, RefCell};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{PtInRect, ScreenToClient};
use windows::Win32::System::Com::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::Rpc::{I_RpcBindingInqLocalClientPID, RPC_S_OK};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_RBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, HRESULT, Ref, implement};

fn primary_down() -> bool {
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let key = if swapped { VK_RBUTTON } else { VK_LBUTTON };
    unsafe { GetAsyncKeyState(key.0 as _) < 0 }
}

fn process_of(window: HWND) -> u32 {
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process)) };
    process
}

fn window_info(window: HWND) -> String {
    if window.is_invalid() {
        return "none".into();
    }
    let root = unsafe { GetAncestor(window, GA_ROOT) };
    let process = process_of(window);
    format!(
        "{:?} '{}' in root {:?} '{}', {} (pid {process})",
        window.0,
        class_name(window),
        root.0,
        class_name(root),
        process_name(process)
    )
}

fn cursor_info() -> String {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return "cursor unknown".into();
    }
    format!("cursor ({}, {}) over {}", point.x, point.y, window_info(unsafe { WindowFromPoint(point) }))
}

fn caller() -> Option<u32> {
    let mut process = 0;
    let status = unsafe { I_RpcBindingInqLocalClientPID(std::ptr::null_mut(), &mut process) };
    (status == RPC_S_OK && process != 0).then_some(process)
}

fn hit(window: HWND, point: POINT, process: u32) -> bool {
    let mut rect = RECT::default();
    let mut cloaked = 0u32;
    unsafe {
        process_of(window) == process
            && IsWindowVisible(window).as_bool()
            && GetWindowRect(window, &mut rect).is_ok()
            && PtInRect(&rect, point).as_bool()
            && DwmGetWindowAttribute(window, DWMWA_CLOAKED, &mut cloaked as *mut _ as _, 4)
                .map_or(true, |_| cloaked == 0)
    }
}

fn shell_view_at(point: POINT, process: u32) -> bool {
    let Ok(mut window) = (unsafe { GetTopWindow(None) }) else {
        return false;
    };
    while !hit(window, point, process) {
        match unsafe { GetWindow(window, GW_HWNDNEXT) } {
            Ok(next) => window = next,
            Err(_) => return false,
        }
    }
    let root = window;
    loop {
        let mut local = point;
        let _ = unsafe { ScreenToClient(window, &mut local) };
        let child = unsafe { ChildWindowFromPointEx(window, local, CWP_SKIPINVISIBLE | CWP_SKIPTRANSPARENT) };
        if child.is_invalid() || child == window {
            break;
        }
        window = child;
    }
    while !window.is_invalid() {
        if class_name(window) == "SHELLDLL_DefView" {
            return true;
        }
        if window == root {
            break;
        }
        window = unsafe { GetAncestor(window, GA_PARENT) };
    }
    false
}

fn file_target() -> (bool, String) {
    let Some(process) = caller() else {
        return (true, "the system format list".into());
    };
    let name = process_name(process);
    if name.eq_ignore_ascii_case("explorer.exe") {
        return (true, format!("{name} (pid {process})"));
    }
    let mut point = POINT::default();
    let shell = unsafe { GetCursorPos(&mut point) }.is_ok() && shell_view_at(point, process);
    (shell, format!("{name} (pid {process}), shell view under the cursor: {shell}"))
}

#[implement(IDataObject)]
pub(super) struct Data {
    items: Vec<Item>,
    notes: bool,
    offered: Vec<u16>,
    rendered: RefCell<Vec<(u16, Vec<u8>)>>,
    stored: RefCell<Vec<(u16, Vec<u8>)>>,
    last_trace: RefCell<String>,
}

impl Data {
    pub(super) fn new(items: Vec<Item>) -> anyhow::Result<Self> {
        if items.is_empty() {
            bail!(tr().empty_contents);
        }
        for item in &items {
            if item.path.exists() {
                continue;
            }
            match &item.text {
                Some(text) => store::restore_note(&item.path, text)?,
                None => bail!(fill(tr().missing_original, &item.title)),
            }
        }
        let mut offered = vec![CF_HDROP.0];
        let notes = items.iter().all(|i| i.text.is_some());
        if notes {
            offered.push(CF_UNICODETEXT.0);
        }
        if let [item] = items.as_slice()
            && item.kind == Kind::Image
        {
            offered.push(png_format());
            offered.push(CF_DIB.0);
        }
        Ok(Self {
            items,
            notes,
            offered,
            rendered: RefCell::new(Vec::new()),
            stored: RefCell::new(Vec::new()),
            last_trace: RefCell::new(String::new()),
        })
    }

    fn render(&self, cf: u16) -> Option<Vec<u8>> {
        if let Some((_, bytes)) = self.stored.borrow().iter().find(|f| f.0 == cf) {
            return Some(bytes.clone());
        }
        if !self.offers(cf) {
            return None;
        }
        if let Some((_, bytes)) = self.rendered.borrow().iter().find(|f| f.0 == cf) {
            return Some(bytes.clone());
        }
        let bytes = if cf == CF_HDROP.0 {
            hdrop(&self.items)
        } else if cf == CF_UNICODETEXT.0 {
            let text = self.items.iter().filter_map(|i| i.shown_text()).collect::<Vec<_>>().join("\r\n");
            text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
        } else if cf == png_format() {
            store::png_bytes(&self.items.first()?.path).ok()?
        } else if cf == CF_DIB.0 {
            store::dib_bytes(&self.items.first()?.path).ok()?
        } else {
            return None;
        };
        self.rendered.borrow_mut().push((cf, bytes.clone()));
        Some(bytes)
    }

    fn offers(&self, cf: u16) -> bool {
        if !self.offered.contains(&cf) {
            return false;
        }
        if cf != CF_HDROP.0 || !self.notes {
            return true;
        }
        let (allowed, target) = file_target();
        self.trace(format!("text file {} to {target}", if allowed { "offered" } else { "hidden" }));
        allowed
    }

    fn trace(&self, line: String) {
        let mut last = self.last_trace.borrow_mut();
        if *last != line {
            log::debug!("Drag out: {line}");
            *last = line;
        }
    }

    fn has(&self, cf: u16) -> bool {
        self.offers(cf) || self.stored.borrow().iter().any(|f| f.0 == cf)
    }
}

fn hdrop(items: &[Item]) -> Vec<u8> {
    let mut files = Vec::new();
    files.extend_from_slice(&20u32.to_le_bytes());
    files.extend_from_slice(&[0u8; 12]);
    files.extend_from_slice(&1u32.to_le_bytes());
    for item in items {
        for word in item.path.as_os_str().to_string_lossy().encode_utf16().chain([0]) {
            files.extend_from_slice(&word.to_le_bytes());
        }
    }
    files.extend_from_slice(&[0u8; 2]);
    files
}

fn medium_from_bytes(bytes: &[u8]) -> windows::core::Result<STGMEDIUM> {
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1))?;
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            let _ = GlobalFree(Some(handle));
            return Err(windows::core::Error::from_win32());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as _, bytes.len());
        let _ = GlobalUnlock(handle);
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: handle },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        })
    }
}

impl IDataObject_Impl for Data_Impl {
    fn GetData(&self, fmt: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        guarded(Err(E_UNEXPECTED.into()), || {
            if self.QueryGetData(fmt) != S_OK {
                return Err(DV_E_FORMATETC.into());
            }
            let cf = unsafe { (*fmt).cfFormat };
            let bytes = self.render(cf).ok_or(windows::core::Error::from(DV_E_FORMATETC))?;
            medium_from_bytes(&bytes)
        })
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, fmt: *const FORMATETC) -> HRESULT {
        guarded(E_UNEXPECTED, || {
            let Some(fmt) = (unsafe { fmt.as_ref() }) else {
                return E_INVALIDARG;
            };
            if fmt.dwAspect != DVASPECT_CONTENT.0 {
                return DV_E_DVASPECT;
            }
            if fmt.tymed & TYMED_HGLOBAL.0 as u32 == 0 {
                return DV_E_TYMED;
            }
            if self.has(fmt.cfFormat) { S_OK } else { DV_E_FORMATETC }
        })
    }

    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, out: *mut FORMATETC) -> HRESULT {
        if let Some(out) = unsafe { out.as_mut() } {
            out.ptd = std::ptr::null_mut();
        }
        E_NOTIMPL
    }

    fn SetData(&self, fmt: *const FORMATETC, medium: *const STGMEDIUM, release: BOOL) -> windows::core::Result<()> {
        guarded(Err(E_UNEXPECTED.into()), || {
            let (Some(fmt), Some(medium)) = (unsafe { fmt.as_ref() }, unsafe { medium.as_ref() }) else {
                return Err(E_INVALIDARG.into());
            };
            if medium.tymed != TYMED_HGLOBAL.0 as u32 {
                return Err(DV_E_TYMED.into());
            }
            let bytes = unsafe { global_bytes(medium.u.hGlobal) }.unwrap_or_default();
            if release.as_bool() {
                unsafe { ReleaseStgMedium(medium as *const _ as *mut _) };
            }
            let mut stored = self.stored.borrow_mut();
            stored.retain(|f| f.0 != fmt.cfFormat);
            stored.push((fmt.cfFormat, bytes));
            Ok(())
        })
    }

    fn EnumFormatEtc(&self, direction: u32) -> windows::core::Result<IEnumFORMATETC> {
        guarded(Err(E_UNEXPECTED.into()), || {
            if direction != DATADIR_GET.0 as u32 {
                return Err(E_NOTIMPL.into());
            }
            let formats: Vec<_> = self
                .offered
                .iter()
                .copied()
                .filter(|cf| self.offers(*cf))
                .chain(self.stored.borrow().iter().map(|f| f.0))
                .map(|cf| format(cf, TYMED_HGLOBAL.0 as u32))
                .collect();
            unsafe { SHCreateStdEnumFmtEtc(&formats) }
        })
    }

    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<IAdviseSink>) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

#[implement(IDropSource)]
struct Source {
    feedback: Cell<Option<DROPEFFECT>>,
}

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            log::debug!("Drag out: cancelled with Esc; {}", cursor_info());
            DRAGDROP_S_CANCEL
        } else if keys.0 & MK_RBUTTON.0 != 0 {
            log::debug!("Drag out: cancelled with the right button; {}", cursor_info());
            DRAGDROP_S_CANCEL
        } else if keys.0 & MK_LBUTTON.0 == 0 {
            log::debug!("Drag out: button released, dropping; {}", cursor_info());
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, effect: DROPEFFECT) -> HRESULT {
        if self.feedback.replace(Some(effect)) != Some(effect) {
            log::debug!("Drag out: target effect 0x{:X}; {}", effect.0, cursor_info());
        }
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

struct Exporting;

impl Exporting {
    fn start() -> Self {
        EXPORTING.store(true, Ordering::Relaxed);
        Self
    }
}

impl Drop for Exporting {
    fn drop(&mut self) {
        EXPORTING.store(false, Ordering::Relaxed);
    }
}

pub fn drag(items: Vec<Item>) -> anyhow::Result<()> {
    let result = export(items);
    send(Event::ExportEnd);
    result
}

fn export(items: Vec<Item>) -> anyhow::Result<()> {
    if !primary_down() {
        log::debug!("Drag out skipped: the button was released before it started");
        return Ok(());
    }
    let count = items.len();
    let data: IDataObject = Data::new(items)?.into();
    let source: IDropSource = Source { feedback: Cell::new(None) }.into();
    log::debug!("Dragging {count} item(s) out of Traydex");
    let _exporting = Exporting::start();
    let result = unsafe { SHDoDragDrop(Some(panel()), &data, &source, DROPEFFECT_COPY) };
    match &result {
        Ok(effect) => log::debug!("Drag out finished, effect 0x{:X}", effect.0),
        Err(error) => log::debug!("Drag out ended: {error}"),
    }
    result.map(|_| ()).map_err(Into::into)
}

#[cfg(test)]
mod tests;
