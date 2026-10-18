use super::{Event, guarded, send, window::panel};
use anyhow::Context;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    sync::OnceLock,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
        System::{Com::*, LibraryLoader::GetModuleHandleW, Ole::IObjectWithSite},
        UI::{
            Input::KeyboardAndMouse::{GetActiveWindow, GetFocus, SetFocus, VK_ESCAPE},
            Shell::{PropertiesSystem::*, *},
            WindowsAndMessaging::*,
        },
    },
    core::{GUID, HSTRING, Interface, PWSTR, Result, implement, w},
};

const CLASS: windows::core::PCWSTR = w!("TraydexPreview");
const RESIZE: u32 = WM_APP + 1;
const CLOSE: u32 = WM_APP + 2;

pub fn preview_handler(path: &Path) -> Option<GUID> {
    let extension = HSTRING::from(format!(".{}", path.extension()?.to_string_lossy()));
    let mut buffer = [0u16; 64];
    let mut length = buffer.len() as u32;
    unsafe {
        AssocQueryStringW(
            ASSOCF_INIT_DEFAULTTOSTAR,
            ASSOCSTR_SHELLEXTENSION,
            &extension,
            w!("{8895b1c6-b41f-4c1c-a562-0d564250836f}"),
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut length,
        )
        .ok()
        .ok()?;
        CLSIDFromString(windows::core::PCWSTR(buffer.as_ptr())).ok()
    }
}

pub fn type_name(path: &Path) -> Option<String> {
    let mut info = SHFILEINFOW::default();
    let found = unsafe {
        SHGetFileInfoW(
            &HSTRING::from(path),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_TYPENAME,
        )
    };
    let name = info.szTypeName;
    let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    (found != 0 && end > 0).then(|| String::from_utf16_lossy(&name[..end]))
}

pub fn thumbnail(path: &Path, size: i32) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(path), None::<&IBindCtx>).ok()?;
        let bitmap = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_RESIZETOFIT).ok()?;
        let pixels = bitmap_pixels(bitmap);
        let _ = DeleteObject(bitmap.into());
        pixels
    }
}

unsafe fn bitmap_pixels(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut header = BITMAP::default();
    let read = unsafe { GetObjectW(bitmap.into(), size_of::<BITMAP>() as i32, Some(&mut header as *mut BITMAP as _)) };
    let (width, height) = (header.bmWidth, header.bmHeight.abs());
    if read == 0 || width <= 0 || height <= 0 {
        return None;
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let rows = unsafe {
        let dc = GetDC(None);
        let rows = GetDIBits(dc, bitmap, 0, height as u32, Some(pixels.as_mut_ptr() as _), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        rows
    };
    if rows == 0 {
        return None;
    }
    if pixels.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0) {
        pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 255);
    }
    Some((width as u32, height as u32, pixels))
}

pub fn focus() {
    unsafe {
        let _ = SetFocus(Some(panel()));
    }
}

struct State {
    handler: RefCell<Option<IPreviewHandler>>,
    brush: HBRUSH,
    busy: Cell<bool>,
    resize: Cell<bool>,
    closing: Cell<bool>,
}

pub struct Host {
    hwnd: HWND,
    rect: Cell<RECT>,
}

impl Host {
    pub fn open(clsid: GUID, path: &Path, rect: RECT, background: u32, text: u32) -> anyhow::Result<Self> {
        let host = Self { hwnd: create(rect, background)?, rect: Cell::new(rect) };
        let handler: IPreviewHandler = unsafe {
            CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)
                .or_else(|_| CoCreateInstance(&clsid, None, CLSCTX_INPROC_SERVER))?
        };
        initialize(&handler, path)?;
        unsafe {
            if let Ok(visuals) = handler.cast::<IPreviewHandlerVisuals>() {
                let _ = visuals.SetBackgroundColor(COLORREF(background));
                let _ = visuals.SetTextColor(COLORREF(text));
            }
            if let Ok(site) = handler.cast::<IObjectWithSite>() {
                let frame: IPreviewHandlerFrame = Frame.into();
                let _ = site.SetSite(&frame);
            }
        }
        let state = state(host.hwnd).context("preview window is gone")?;
        *state.handler.borrow_mut() = Some(handler.clone());
        state.busy.set(true);
        let shown = unsafe { handler.SetWindow(host.hwnd, &client(host.hwnd)).and_then(|_| handler.DoPreview()) };
        state.busy.set(false);
        flush(host.hwnd, state);
        shown?;
        focus();
        Ok(host)
    }

    pub fn place(&self, rect: RECT) {
        if rect == self.rect.get() {
            return;
        }
        self.rect.set(rect);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            let _ = PostMessageW(Some(self.hwnd), CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

fn initialize(handler: &IPreviewHandler, path: &Path) -> Result<()> {
    let path = HSTRING::from(path);
    unsafe {
        if let Ok(init) = handler.cast::<IInitializeWithFile>() {
            return init.Initialize(&path, STGM_READ.0);
        }
        if let Ok(init) = handler.cast::<IInitializeWithItem>() {
            let item: IShellItem = SHCreateItemFromParsingName(&path, None::<&IBindCtx>)?;
            return init.Initialize(&item, STGM_READ.0);
        }
        let init = handler.cast::<IInitializeWithStream>()?;
        let stream = SHCreateStreamOnFileEx(&path, (STGM_READ | STGM_SHARE_DENY_NONE).0, 0, false, None)?;
        init.Initialize(&stream, STGM_READ.0)
    }
}

fn register() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return false;
        };
        let class = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&class) != 0
    })
}

fn create(rect: RECT, background: u32) -> anyhow::Result<HWND> {
    anyhow::ensure!(register(), "preview window class is not registered");
    let state = Box::into_raw(Box::new(State {
        handler: RefCell::new(None),
        brush: unsafe { CreateSolidBrush(COLORREF(background)) },
        busy: Cell::new(false),
        resize: Cell::new(false),
        closing: Cell::new(false),
    }));
    let created = unsafe {
        CreateWindowExW(
            WS_EX_NOPARENTNOTIFY,
            CLASS,
            None,
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            Some(panel()),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            Some(state as _),
        )
    };
    if created.is_err() {
        let state = unsafe { Box::from_raw(state) };
        let _ = unsafe { DeleteObject(state.brush.into()) };
    }
    Ok(created?)
}

fn state<'a>(hwnd: HWND) -> Option<&'a State> {
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref() }
}

fn client(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    rect
}

fn flush(hwnd: HWND, state: &State) {
    let message = if state.closing.get() {
        CLOSE
    } else if state.resize.take() {
        RESIZE
    } else {
        return;
    };
    unsafe {
        let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
    }
}

fn run(hwnd: HWND, state: &State, flag: &Cell<bool>, job: impl FnOnce(&IPreviewHandler) -> Result<()>) {
    if state.busy.get() {
        flag.set(true);
        return;
    }
    let Some(handler) = state.handler.borrow().clone() else {
        return;
    };
    state.busy.set(true);
    if let Err(error) = job(&handler) {
        log::debug!("Preview handler call failed: {error}");
    }
    state.busy.set(false);
    flush(hwnd, state);
}

fn close(hwnd: HWND, state: &State) {
    if state.busy.get() {
        state.closing.set(true);
        return;
    }
    if let Some(handler) = state.handler.take() {
        state.busy.set(true);
        unsafe {
            if let Ok(site) = handler.cast::<IObjectWithSite>() {
                let _ = site.SetSite(None);
            }
            let _ = handler.Unload();
        }
        state.busy.set(false);
    }
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    reclaim();
}

fn reclaim() {
    let panel = panel();
    unsafe {
        let focus = GetFocus();
        if GetActiveWindow() == panel && focus != panel && !IsChild(panel, focus).as_bool() {
            let _ = SetFocus(Some(panel));
        }
    }
}

unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    guarded(None, || handle(hwnd, msg, wp, lp)).unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wp, lp) })
}

fn handle(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    if msg == WM_NCCREATE {
        let create = unsafe { (lp.0 as *const CREATESTRUCTW).as_ref() }?;
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as _) };
        return None;
    }
    let state = state(hwnd)?;
    match msg {
        WM_ERASEBKGND => {
            unsafe { FillRect(HDC(wp.0 as _), &client(hwnd), state.brush) };
            Some(LRESULT(1))
        }
        WM_SIZE => {
            unsafe {
                let _ = PostMessageW(Some(hwnd), RESIZE, WPARAM(0), LPARAM(0));
            }
            None
        }
        RESIZE => {
            run(hwnd, state, &state.resize, |handler| unsafe { handler.SetRect(&client(hwnd)) });
            Some(LRESULT(0))
        }
        CLOSE => {
            close(hwnd, state);
            Some(LRESULT(0))
        }
        WM_NCDESTROY => {
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            let state = unsafe { Box::from_raw(state as *const State as *mut State) };
            let _ = unsafe { DeleteObject(state.brush.into()) };
            None
        }
        _ => None,
    }
}

fn accelerators() -> HACCEL {
    static TABLE: OnceLock<isize> = OnceLock::new();
    HACCEL(*TABLE.get_or_init(|| unsafe {
        let keys = [ACCEL { fVirt: FVIRTKEY, key: VK_ESCAPE.0, cmd: 1 }];
        CreateAcceleratorTableW(&keys).map(|table| table.0 as isize).unwrap_or_default()
    }) as _)
}

#[implement(IPreviewHandlerFrame)]
struct Frame;

impl IPreviewHandlerFrame_Impl for Frame_Impl {
    fn GetWindowContext(&self) -> Result<PREVIEWHANDLERFRAMEINFO> {
        Ok(PREVIEWHANDLERFRAMEINFO { haccel: accelerators(), cAccelEntries: 1 })
    }

    fn TranslateAccelerator(&self, msg: *const MSG) -> Result<()> {
        let escape = unsafe { msg.as_ref() }
            .is_some_and(|msg| msg.message == WM_KEYDOWN && msg.wParam.0 == VK_ESCAPE.0 as usize);
        if !escape {
            return Err(S_FALSE.into());
        }
        send(Event::Escape);
        Ok(())
    }
}
