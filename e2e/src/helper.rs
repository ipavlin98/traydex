use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc;
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL},
        LibraryLoader::GetModuleHandleW,
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        Ole::{CF_HDROP, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, IDropSource, OleInitialize},
    },
    UI::{
        Shell::{SHCreateDataObject, SHDoDragDrop},
        WindowsAndMessaging::*,
    },
};
use windows::core::{HSTRING, w};

pub enum Payload {
    Text(String),
    Files(Vec<PathBuf>),
}

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static ARMED: Mutex<Option<Payload>> = Mutex::new(None);
static STATUS: Mutex<String> = Mutex::new(String::new());
static EFFECT: AtomicU32 = AtomicU32::new(u32::MAX);
static RIGHT_BUTTON: AtomicU32 = AtomicU32::new(0);
const REPAINT: u32 = WM_APP + 1;

pub fn window() -> HWND {
    HWND(WINDOW.load(Ordering::SeqCst) as _)
}

pub fn status(text: &str) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = text.to_string();
    unsafe {
        let _ = PostMessageW(Some(window()), REPAINT, WPARAM(0), LPARAM(0));
    }
}

pub fn arm(payload: Payload) {
    EFFECT.store(u32::MAX, Ordering::SeqCst);
    RIGHT_BUTTON.store(0, Ordering::SeqCst);
    *ARMED.lock().unwrap_or_else(|e| e.into_inner()) = Some(payload);
}

pub fn disarm() {
    ARMED.lock().unwrap_or_else(|e| e.into_inner()).take();
}

pub fn right_button_messages() -> u32 {
    RIGHT_BUTTON.load(Ordering::SeqCst)
}

pub fn effect() -> Option<u32> {
    let effect = EFFECT.load(Ordering::SeqCst);
    (effect != u32::MAX).then_some(effect)
}

fn global(bytes: &[u8]) -> Option<HGLOBAL> {
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).ok()?;
        let pointer = GlobalLock(handle) as *mut u8;
        if pointer.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
        let _ = GlobalUnlock(handle);
        Some(handle)
    }
}

fn wide_bytes(units: impl Iterator<Item = u16>) -> Vec<u8> {
    units.flat_map(u16::to_le_bytes).collect()
}

fn encode(payload: &Payload) -> (u16, Vec<u8>) {
    match payload {
        Payload::Text(text) => (CF_UNICODETEXT.0, wide_bytes(text.encode_utf16().chain([0]))),
        Payload::Files(files) => {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&20u32.to_le_bytes());
            bytes.extend_from_slice(&[0; 12]);
            bytes.extend_from_slice(&1u32.to_le_bytes());
            let list = files
                .iter()
                .flat_map(|file| file.as_os_str().to_string_lossy().encode_utf16().chain([0]).collect::<Vec<_>>())
                .chain([0]);
            bytes.extend(wide_bytes(list));
            (CF_HDROP.0, bytes)
        }
    }
}

fn data_object(payload: &Payload) -> windows::core::Result<IDataObject> {
    let data: IDataObject = unsafe { SHCreateDataObject(None, None, None::<&IDataObject>)? };
    let (format, bytes) = encode(payload);
    let handle = global(&bytes).ok_or_else(windows::core::Error::from_win32)?;
    let format = FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let medium = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: handle },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    };
    unsafe { data.SetData(&format, &medium, true)? };
    Ok(data)
}

fn start_drag(hwnd: HWND) {
    let Some(payload) = ARMED.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    let Ok(data) = data_object(&payload) else {
        EFFECT.store(0, Ordering::SeqCst);
        return;
    };
    let effect =
        unsafe { SHDoDragDrop(Some(hwnd), &data, None::<&IDropSource>, DROPEFFECT_COPY) }.unwrap_or(DROPEFFECT(0));
    EFFECT.store(effect.0, Ordering::SeqCst);
}

fn paint(hwnd: HWND) {
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut paint);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let brush = CreateSolidBrush(COLORREF(0x00_30_20_10));
        FillRect(dc, &rect, brush);
        let _ = DeleteObject(brush.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00_ff_ff_ff));
        rect.left += 16;
        rect.top += 14;
        rect.right -= 16;
        let text = format!(
            "Traydex E2E: mouse and keyboard are blocked by the test.\nStop and get input back: {}\n\n{}",
            crate::guard::STOP_CHORD,
            STATUS.lock().unwrap_or_else(|e| e.into_inner())
        );
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(dc, &mut wide, &mut rect, DT_WORDBREAK | DT_LEFT | DT_TOP);
        let _ = EndPaint(hwnd, &paint);
    }
}

unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            start_drag(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_RBUTTONDOWN | WM_RBUTTONUP | WM_CONTEXTMENU => {
            RIGHT_BUTTON.fetch_add(1, Ordering::SeqCst);
            LRESULT(0)
        }
        REPAINT => {
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_ACTIVATE as isize),
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

pub fn start(rect: RECT) -> anyhow::Result<HWND> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new().name("e2e-helper".into()).spawn(move || unsafe {
        let _ = OleInitialize(None);
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: w!("Traydex.E2E.Helper"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&class);
        let created = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("Traydex.E2E.Helper"),
            &HSTRING::from("Traydex E2E"),
            WS_POPUP | WS_BORDER,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        );
        let Ok(hwnd) = created else {
            let _ = tx.send(None);
            return;
        };
        WINDOW.store(hwnd.0 as isize, Ordering::SeqCst);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = tx.send(Some(hwnd.0 as isize));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    })?;
    let hwnd = rx.recv()?.ok_or_else(|| anyhow::anyhow!("couldn't create the helper window"))?;
    Ok(HWND(hwnd as _))
}

pub fn stop() {
    unsafe {
        let _ = PostMessageW(Some(window()), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
