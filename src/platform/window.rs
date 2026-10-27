mod geometry;

use self::geometry::{anchored, beside, edge, edge_code, region, stretch};
use super::{Event, corner, guarded, send};
use crate::store::MIN_SIZE;
use crate::system::Caption;
use std::{
    cell::Cell,
    sync::atomic::{AtomicIsize, AtomicU32, Ordering},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{
                GetActiveWindow, GetFocus, GetKeyState, INPUT, INPUT_MOUSE, SendInput, VIRTUAL_KEY, VK_CONTROL,
                VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, HSTRING},
};

static PANEL: AtomicIsize = AtomicIsize::new(0);
static ORIGINAL: AtomicIsize = AtomicIsize::new(0);
static MIN_WIDTH: AtomicU32 = AtomicU32::new(0);
static MIN_HEIGHT: AtomicU32 = AtomicU32::new(0);
const SNAP: i32 = 10;
const CURSOR_GAP: i32 = 80;
const NO_BORDER: u32 = 0xFFFF_FFFE;
const FORCE_REDRAW: u32 = WM_USER + 5;

#[derive(Clone, Copy)]
struct Gesture {
    cursor: POINT,
    rect: RECT,
}

thread_local! {
    static MOVING: Cell<bool> = const { Cell::new(false) };
    static GESTURE: Cell<Option<Gesture>> = const { Cell::new(None) };
    static CAPTION: Cell<Caption> = Cell::new(Caption::default());
    static KEY_TRACED: Cell<bool> = const { Cell::new(false) };
    static CHAR_TRACED: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn panel() -> HWND {
    HWND(PANEL.load(Ordering::Relaxed) as _)
}

pub(super) fn attach(hwnd: HWND, hidden: bool) {
    PANEL.store(hwnd.0 as isize, Ordering::Relaxed);
    unsafe {
        let corner = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as _,
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
        let disabled = BOOL::from(true);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &disabled as *const _ as _,
            size_of::<BOOL>() as u32,
        );
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &NO_BORDER as *const _ as _, size_of::<u32>() as u32);
        let _ = SetWindowTextW(hwnd, &HSTRING::from(crate::profile::current().name()));
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            ((style & WS_VISIBLE.0) | WS_POPUP.0 | WS_CLIPSIBLINGS.0 | WS_CLIPCHILDREN.0) as _,
        );
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            (ex_style & !(WS_EX_WINDOWEDGE.0 | WS_EX_DLGMODALFRAME.0 | WS_EX_CLIENTEDGE.0)) as _,
        );
        let original = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, panel_proc as *const () as _);
        ORIGINAL.store(original as _, Ordering::Relaxed);
        if hidden {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

const GRIP: i32 = 6;
const CORNER: i32 = 16;

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    guarded((), || trace(msg, wp));
    guarded(None, || intercept(hwnd, msg, wp, lp)).unwrap_or_else(|| forward(hwnd, msg, wp, lp))
}

fn trace(msg: u32, wp: WPARAM) {
    let name = match msg {
        WM_ACTIVATE => "WM_ACTIVATE",
        WM_ACTIVATEAPP => "WM_ACTIVATEAPP",
        WM_MOUSEACTIVATE => "WM_MOUSEACTIVATE",
        WM_SETFOCUS => "WM_SETFOCUS",
        WM_KILLFOCUS => "WM_KILLFOCUS",
        WM_KEYDOWN if !KEY_TRACED.replace(true) => "first WM_KEYDOWN",
        WM_SYSKEYDOWN if !KEY_TRACED.replace(true) => "first WM_SYSKEYDOWN",
        WM_CHAR if !CHAR_TRACED.replace(true) => "first WM_CHAR",
        _ => return,
    };
    if matches!(msg, WM_ACTIVATE | WM_SETFOCUS | WM_KILLFOCUS) {
        KEY_TRACED.set(false);
        CHAR_TRACED.set(false);
    }
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => log::debug!("Panel {name}: {}, {}", input_state(), held_modifiers()),
        WM_CHAR => log::debug!("Panel {name}: {}", input_state()),
        _ => log::debug!("Panel {name} {:#x}: {}", wp.0, input_state()),
    }
}

fn held_modifiers() -> String {
    let keys: [(VIRTUAL_KEY, &str); 5] =
        [(VK_CONTROL, "ctrl"), (VK_MENU, "alt"), (VK_SHIFT, "shift"), (VK_LWIN, "win"), (VK_RWIN, "win")];
    let held: Vec<&str> =
        keys.iter().filter(|(key, _)| unsafe { GetKeyState(key.0 as i32) } < 0).map(|(_, name)| *name).collect();
    format!("modifiers held: {}", if held.is_empty() { "none".to_string() } else { held.join(" ") })
}

fn input_state() -> String {
    let name = |hwnd: HWND| match hwnd {
        hwnd if hwnd.is_invalid() => "none".to_string(),
        hwnd if hwnd == panel() => "panel".to_string(),
        hwnd => format!("{:?}", hwnd.0),
    };
    unsafe {
        format!(
            "focus {}, active {}, foreground {}",
            name(GetFocus()),
            name(GetActiveWindow()),
            name(GetForegroundWindow())
        )
    }
}

fn intercept(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_NCCALCSIZE => Some(LRESULT(0)),
        WM_NCHITTEST => Some(LRESULT(hit_test(hwnd, lp) as isize)),
        WM_NCLBUTTONDOWN if is_frame(wp.0 as u32) => Some(unsafe { DefWindowProcW(hwnd, msg, wp, lp) }),
        WM_NCLBUTTONDBLCLK if wp.0 as u32 == HTCAPTION => {
            send(Event::ResetSize);
            Some(LRESULT(0))
        }
        WM_NCRBUTTONDBLCLK if wp.0 as u32 == HTCAPTION => {
            send(Event::FitSize);
            Some(LRESULT(0))
        }
        WM_NCLBUTTONDBLCLK | WM_NCRBUTTONDOWN | WM_NCRBUTTONUP if is_frame(wp.0 as u32) => Some(LRESULT(0)),
        WM_ENTERSIZEMOVE => {
            MOVING.set(true);
            let mut cursor = POINT::default();
            let mut rect = RECT::default();
            unsafe {
                let _ = GetCursorPos(&mut cursor);
                let _ = GetWindowRect(hwnd, &mut rect);
            }
            GESTURE.set(Some(Gesture { cursor, rect }));
            None
        }
        WM_MOVING => {
            if let (Some(rect), Some(gesture)) = (unsafe { (lp.0 as *mut RECT).as_mut() }, GESTURE.get()) {
                let (dx, dy) = cursor_delta(gesture);
                *rect = gesture.rect;
                rect.left += dx;
                rect.right += dx;
                rect.top += dy;
                rect.bottom += dy;
                snap(hwnd, rect);
            }
            Some(LRESULT(1))
        }
        WM_SIZING => {
            if let (Some(rect), Some(gesture)) = (unsafe { (lp.0 as *mut RECT).as_mut() }, GESTURE.get()) {
                resize_edges(hwnd, wp.0 as u32, gesture, rect);
            }
            Some(LRESULT(1))
        }
        WM_EXITSIZEMOVE => {
            MOVING.set(false);
            GESTURE.set(None);
            let origin = client_origin(hwnd);
            let (width, height) = client_size(hwnd);
            let scale = scale(hwnd);
            send(Event::Placed {
                x: origin.x,
                y: origin.y,
                width: width as f32 / scale,
                height: height as f32 / scale,
            });
            None
        }
        WM_MOVE if !MOVING.get() && !unsafe { IsWindowVisible(hwnd) }.as_bool() => Some(LRESULT(0)),
        _ => None,
    }
}

fn is_frame(code: u32) -> bool {
    matches!(code, HTCAPTION | HTTOP | HTBOTTOM | HTTOPLEFT | HTTOPRIGHT | HTBOTTOMLEFT | HTBOTTOMRIGHT)
}

fn hit_test(hwnd: HWND, lp: LPARAM) -> u32 {
    let mut point = POINT { x: (lp.0 & 0xffff) as i16 as i32, y: ((lp.0 >> 16) & 0xffff) as i16 as i32 };
    if !corner::at_tip(point)
        && let Some(code) = edge_hit(hwnd, point)
    {
        return code;
    }
    unsafe {
        let _ = ScreenToClient(hwnd, &mut point);
    }
    let (width, _) = client_size(hwnd);
    let caption = CAPTION.get();
    let span = |value: f32| scaled(hwnd, value as i32);
    let leading = span(caption.leading.0.floor())..span(caption.leading.1.ceil());
    let trailing = width - span(caption.trailing.ceil());
    region(point, span(caption.height.ceil()), leading, trailing)
}

fn edge_hit(hwnd: HWND, point: POINT) -> Option<u32> {
    let origin = client_origin(hwnd);
    let (width, height) = client_size(hwnd);
    let (grip, corner) = (scaled(hwnd, GRIP), scaled(hwnd, CORNER));
    let (left, top) = (point.x - origin.x, point.y - origin.y);
    let (right, bottom) = (origin.x + width - 1 - point.x, origin.y + height - 1 - point.y);
    edge_code(left, top, right, bottom, grip, corner)
}

fn forward(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let original: WNDPROC = std::mem::transmute(ORIGINAL.load(Ordering::Relaxed));
        CallWindowProcW(original, hwnd, msg, wp, lp)
    }
}

fn client_origin(hwnd: HWND) -> POINT {
    let mut point = POINT::default();
    unsafe {
        let _ = ClientToScreen(hwnd, &mut point);
    }
    point
}

fn client_size(hwnd: HWND) -> (i32, i32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn frame_offset(hwnd: HWND) -> (i32, i32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    let origin = client_origin(hwnd);
    (origin.x - rect.left, origin.y - rect.top)
}

fn scale(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.
}

pub(super) fn client_point(x: i32, y: i32) -> (f32, f32) {
    let hwnd = panel();
    let origin = client_origin(hwnd);
    let scale = scale(hwnd);
    ((x - origin.x) as f32 / scale, (y - origin.y) as f32 / scale)
}

fn scaled(hwnd: HWND, value: i32) -> i32 {
    (value as f32 * scale(hwnd)).round() as i32
}

fn work_area(rect: &RECT, flags: MONITOR_FROM_FLAGS) -> Option<RECT> {
    unsafe {
        let monitor = MonitorFromRect(rect, flags);
        if monitor.is_invalid() {
            return None;
        }
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(monitor, &mut info).as_bool().then_some(info.rcWork)
    }
}

fn snap(hwnd: HWND, rect: &mut RECT) {
    let (left, top) = frame_offset(hwnd);
    let (width, height) = client_size(hwnd);
    let visible = RECT {
        left: rect.left + left,
        top: rect.top + top,
        right: rect.left + left + width,
        bottom: rect.top + top + height,
    };
    let Some(work) = work_area(&visible, MONITOR_DEFAULTTONEAREST) else {
        return;
    };
    let threshold = scaled(hwnd, SNAP);
    let dx = edge(visible.left, visible.right, work.left, work.right, threshold);
    let dy = edge(visible.top, visible.bottom, work.top, work.bottom, threshold);
    rect.left += dx;
    rect.right += dx;
    rect.top += dy;
    rect.bottom += dy;
}

fn cursor_position() -> POINT {
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    cursor
}

fn cursor_work_area(cursor: POINT, flags: MONITOR_FROM_FLAGS) -> Option<RECT> {
    work_area(&RECT { left: cursor.x, top: cursor.y, right: cursor.x + 1, bottom: cursor.y + 1 }, flags)
}

fn cursor_delta(gesture: Gesture) -> (i32, i32) {
    let cursor = cursor_position();
    (cursor.x - gesture.cursor.x, cursor.y - gesture.cursor.y)
}

fn resize_edges(hwnd: HWND, edge: u32, gesture: Gesture, rect: &mut RECT) {
    let minimum = ((min_width() * scale(hwnd)).round() as i32, (min_height() * scale(hwnd)).round() as i32);
    let work = |next: &RECT| work_area(next, MONITOR_DEFAULTTONEAREST);
    *rect = stretch(gesture.rect, edge, cursor_delta(gesture), work, scaled(hwnd, SNAP), minimum);
}

fn client_rect_at(hwnd: HWND, x: i32, y: i32) -> RECT {
    let (width, height) = client_size(hwnd);
    RECT { left: x, top: y, right: x + width, bottom: y + height }
}

fn fits(hwnd: HWND, x: i32, y: i32) -> bool {
    let rect = client_rect_at(hwnd, x, y);
    work_area(&rect, MONITOR_DEFAULTTONULL).is_some_and(|work| covers(&rect, &work, scaled(hwnd, 40)))
}

fn covers(rect: &RECT, work: &RECT, least: i32) -> bool {
    let mut overlap = RECT::default();
    unsafe { IntersectRect(&mut overlap, rect, work) }.as_bool()
        && overlap.right - overlap.left >= (rect.right - rect.left) / 2
        && overlap.bottom - overlap.top >= least
}

fn default_origin() -> (i32, i32) {
    cursor_work_area(cursor_position(), MONITOR_DEFAULTTOPRIMARY).map_or((0, 0), |work| (work.left, work.top))
}

fn move_client(hwnd: HWND, x: i32, y: i32) {
    let (left, top) = frame_offset(hwnd);
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x - left, y - top, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

pub fn prepare(width: f32, height: f32) {
    let hwnd = panel();
    let scale = scale(hwnd);
    let origin = client_origin(hwnd);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            origin.x,
            origin.y,
            (width * scale).round() as i32,
            (height * scale).round() as i32,
            SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn place(position: Option<(i32, i32)>) {
    let hwnd = panel();
    let (x, y) = position.filter(|(x, y)| fits(hwnd, *x, *y)).unwrap_or_else(default_origin);
    move_client(hwnd, x, y);
}

pub fn reset_position() {
    place(None);
}

pub fn place_near_cursor() {
    let hwnd = panel();
    let cursor = cursor_position();
    let Some(work) = cursor_work_area(cursor, MONITOR_DEFAULTTONEAREST) else {
        return;
    };
    let (x, y) = beside(cursor, work, client_size(hwnd), scaled(hwnd, CURSOR_GAP));
    move_client(hwnd, x, y);
}

fn min_width() -> f32 {
    (MIN_WIDTH.load(Ordering::Relaxed) as f32).max(MIN_SIZE.0)
}

fn min_height() -> f32 {
    (MIN_HEIGHT.load(Ordering::Relaxed) as f32).max(MIN_SIZE.1)
}

pub fn set_caption(caption: Caption) {
    CAPTION.set(caption);
}

pub fn set_min_size(width: f32, height: f32) {
    MIN_WIDTH.store(width.ceil() as u32, Ordering::Relaxed);
    MIN_HEIGHT.store(height.ceil() as u32, Ordering::Relaxed);
    let hwnd = panel();
    let scale = scale(hwnd);
    let (current_width, current_height) = client_size(hwnd);
    if (current_width as f32) < (min_width() * scale).round()
        || (current_height as f32) < (min_height() * scale).round()
    {
        resize(current_width as f32 / scale, current_height as f32 / scale);
    }
}

pub fn resize(width: f32, height: f32) {
    let width = width.max(min_width());
    let height = height.max(min_height());
    let hwnd = panel();
    let scale = scale(hwnd);
    let mut window = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut window);
    }
    let requested = (width * scale).round() as i32;
    let target_height = (height * scale).round() as i32;
    let (x, y, target_width) = match work_area(&window, MONITOR_DEFAULTTONEAREST) {
        Some(work) => {
            let target_width = requested.min(work.right - work.left);
            (
                anchored(window.left, window.right, work.left, work.right, target_width),
                anchored(window.top, window.bottom, work.top, work.bottom, target_height),
                target_width,
            )
        }
        None => (window.left, window.top, requested),
    };
    let width = if target_width < requested { target_width as f32 / scale } else { width };
    unsafe {
        let _ = SetWindowPos(hwnd, None, x, y, target_width, target_height, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    let origin = client_origin(hwnd);
    if !fits(hwnd, origin.x, origin.y) {
        let (x, y) = default_origin();
        move_client(hwnd, x, y);
    }
    let origin = client_origin(hwnd);
    send(Event::Placed { x: origin.x, y: origin.y, width, height });
}

pub fn show(activate: bool) {
    let hwnd = panel();
    unsafe {
        let origin = client_origin(hwnd);
        if !fits(hwnd, origin.x, origin.y) {
            let (x, y) = default_origin();
            move_client(hwnd, x, y);
        }
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = ShowWindow(hwnd, if activate { SW_SHOW } else { SW_SHOWNOACTIVATE });
        SendMessageW(hwnd, FORCE_REDRAW, None, None);
        if activate && !SetForegroundWindow(hwnd).as_bool() {
            let input = INPUT { r#type: INPUT_MOUSE, ..Default::default() };
            SendInput(&[input], size_of::<INPUT>() as i32);
            if !SetForegroundWindow(hwnd).as_bool() {
                log::warn!("Couldn't bring the window to the foreground");
            }
        }
    }
    let topmost = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0;
    log::debug!("Window shown: visible {}, topmost {topmost}, {}", visible(), input_state());
}

pub fn hide() {
    unsafe {
        let _ = ShowWindowAsync(panel(), SW_HIDE);
    }
}

pub fn visible() -> bool {
    unsafe { IsWindowVisible(panel()).as_bool() }
}

pub(super) fn contains(point: POINT) -> bool {
    if !visible() {
        return false;
    }
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(panel(), &mut rect);
    }
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

pub fn pointer_inside() -> bool {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    contains(point)
}

#[cfg(test)]
mod tests;
