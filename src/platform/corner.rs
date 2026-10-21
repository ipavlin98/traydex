use super::{
    Event,
    drag::{self, buttons_down, class_name},
    guarded, relay, send, window,
};
use crate::store::Corner;
use std::{
    cell::Cell,
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, POINT, RECT},
    Graphics::Gdi::*,
    Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
    System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION},
    UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
    UI::HiDpi::GetDpiForSystem,
    UI::WindowsAndMessaging::{
        EVENT_SYSTEM_FOREGROUND, GWL_STYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        GetWindowThreadProcessId, IsZoomed, KillTimer, SetTimer, WINEVENT_OUTOFCONTEXT, WS_CAPTION,
    },
};

pub(super) const TIMER: usize = 2;
const POLL_MS: u32 = 30;
const REACH: i32 = 2;
const REARM: i32 = 60;
const DRAG_ZONE: i32 = 440;
const DESKTOP_CLASSES: [&str; 2] = ["Progman", "WorkerW"];

static ACTIVE: AtomicU8 = AtomicU8::new(0);
static DRAG_REVEAL: AtomicBool = AtomicBool::new(false);

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(true) };
    static POLLING: Cell<bool> = const { Cell::new(false) };
}

pub fn set_corner(corner: Option<Corner>) {
    ACTIVE.store(corner.map_or(0, |corner| corner as u8 + 1), Ordering::Relaxed);
}

pub fn set_corner_drag(enabled: bool) {
    DRAG_REVEAL.store(enabled, Ordering::Relaxed);
}

fn active() -> Option<Corner> {
    let index = ACTIVE.load(Ordering::Relaxed).checked_sub(1)?;
    Corner::ALL.get(index as usize).copied()
}

fn screen_at(point: POINT) -> Option<(HMONITOR, RECT)> {
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONULL);
        (!monitor.is_invalid() && GetMonitorInfoW(monitor, &mut info).as_bool()).then_some((monitor, info.rcMonitor))
    }
}

fn tip(corner: Corner, screen: RECT) -> (POINT, POINT) {
    let (x, outward_x) = match corner {
        Corner::TopLeft | Corner::BottomLeft => (screen.left, -1),
        Corner::TopRight | Corner::BottomRight => (screen.right - 1, 1),
    };
    let (y, outward_y) = match corner {
        Corner::TopLeft | Corner::TopRight => (screen.top, -1),
        Corner::BottomLeft | Corner::BottomRight => (screen.bottom - 1, 1),
    };
    (POINT { x, y }, POINT { x: outward_x, y: outward_y })
}

fn distance(point: POINT, tip: POINT) -> i32 {
    (point.x - tip.x).abs().max((point.y - tip.y).abs())
}

pub(super) fn at_tip(point: POINT) -> bool {
    let Some((corner, (_, screen))) = active().zip(screen_at(point)) else {
        return false;
    };
    on_tip(point, corner, screen, outer)
}

fn on_tip(point: POINT, corner: Corner, screen: RECT, outer: impl FnOnce(POINT, POINT) -> bool) -> bool {
    let (tip, outward) = tip(corner, screen);
    distance(point, tip) < REACH && outer(tip, outward)
}

fn decide(
    armed: &mut bool,
    distance: i32,
    rearm: i32,
    inside: impl FnOnce() -> bool,
    outer: impl FnOnce() -> bool,
    held: impl FnOnce() -> bool,
    covered: impl FnOnce() -> bool,
) -> bool {
    if !*armed {
        if distance > rearm || (distance >= REACH && inside()) {
            *armed = true;
        }
        return false;
    }
    if distance >= REACH || !outer() {
        return false;
    }
    *armed = false;
    if held() {
        log::debug!("Hot corner ignored: a mouse button is held");
        return false;
    }
    if covered() {
        log::debug!("Hot corner ignored: a full-screen window is in front");
        return false;
    }
    true
}

fn reveals_drag(
    distance: i32,
    zone: i32,
    dragging: impl FnOnce() -> bool,
    outer: impl FnOnce() -> bool,
    covered: impl FnOnce() -> bool,
) -> bool {
    distance < zone && dragging() && outer() && !covered()
}

fn off_desktop(point: POINT) -> bool {
    unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONULL) }.is_invalid()
}

fn outer(tip: POINT, outward: POINT) -> bool {
    off_desktop(POINT { x: tip.x + outward.x, y: tip.y }) && off_desktop(POINT { x: tip.x, y: tip.y + outward.y })
}

fn fullscreen(monitor: HMONITOR, screen: RECT) -> bool {
    let front = unsafe { GetForegroundWindow() };
    if front.is_invalid() {
        return false;
    }
    let captioned = unsafe { GetWindowLongPtrW(front, GWL_STYLE) } as u32 & WS_CAPTION.0 == WS_CAPTION.0;
    if captioned && unsafe { IsZoomed(front) }.as_bool() {
        return false;
    }
    if unsafe { MonitorFromWindow(front, MONITOR_DEFAULTTONULL) } != monitor {
        return false;
    }
    if DESKTOP_CLASSES.contains(&class_name(front).as_str()) {
        return false;
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(front, &mut rect) }.is_ok()
        && rect.left <= screen.left
        && rect.top <= screen.top
        && rect.right >= screen.right
        && rect.bottom >= screen.bottom
}

pub(super) fn feed(point: POINT) {
    let Some(corner) = active() else {
        return ARMED.set(true);
    };
    let Some((monitor, screen)) = screen_at(point) else {
        return;
    };
    let (tip, outward) = tip(corner, screen);
    let dpi = unsafe { GetDpiForSystem() }.max(96) as i32;
    let covered = || !window::visible() && fullscreen(monitor, screen);
    if DRAG_REVEAL.load(Ordering::Relaxed)
        && reveals_drag(
            distance(point, tip),
            DRAG_ZONE * dpi / 96,
            || buttons_down() && drag::unseen(),
            || outer(tip, outward),
            covered,
        )
    {
        log::debug!("A drag entered the hot corner area");
        drag::mark_seen();
        return send(Event::DragToCorner);
    }
    let mut armed = ARMED.get();
    let fire = decide(
        &mut armed,
        distance(point, tip),
        REARM * dpi / 96,
        || window::contains(point),
        || outer(tip, outward),
        buttons_down,
        covered,
    );
    ARMED.set(armed);
    if fire {
        log::debug!("Cursor reached the hot corner");
        send(Event::Corner);
    }
}

fn elevated(process: HANDLE) -> Option<bool> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut length = 0;
    let queried = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
    };
    unsafe {
        let _ = CloseHandle(token);
    }
    queried.ok()?;
    Some(elevation.TokenIsElevated != 0)
}

fn owner_elevated(window: HWND) -> Option<bool> {
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process)) };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) }.ok()?;
    let elevated = elevated(handle);
    unsafe {
        let _ = CloseHandle(handle);
    }
    elevated
}

fn follow(front: HWND) {
    if front.is_invalid() {
        return;
    }
    let poll = owner_elevated(front) != Some(false);
    if POLLING.replace(poll) == poll {
        return;
    }
    if poll {
        log::debug!("An elevated window is in front, the hot corner follows the cursor by polling");
        unsafe { SetTimer(Some(relay::window()), TIMER, POLL_MS, None) };
    } else {
        log::debug!("The elevated window left the front, cursor polling stopped");
        unsafe {
            let _ = KillTimer(Some(relay::window()), TIMER);
        }
    }
}

unsafe extern "system" fn foreground_changed(_: HWINEVENTHOOK, _: u32, front: HWND, _: i32, _: i32, _: u32, _: u32) {
    guarded((), || follow(front));
}

pub(super) fn tick() {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_ok() {
        feed(point);
    }
}

pub(super) fn watch() -> Option<HWINEVENTHOOK> {
    if elevated(unsafe { GetCurrentProcess() }) == Some(true) {
        return None;
    }
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.is_invalid() {
        log::warn!("Couldn't follow the foreground window, the hot corner won't work over elevated windows");
        return None;
    }
    follow(unsafe { GetForegroundWindow() });
    Some(hook)
}

pub(super) fn unwatch(hook: HWINEVENTHOOK) {
    unsafe {
        let _ = UnhookWinEvent(hook);
    }
}

#[cfg(test)]
mod tests;
