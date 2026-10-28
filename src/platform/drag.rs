use super::{Event, corner, exporting, guarded, relay, send, window};
use crate::store::ShakeLevel;
use std::{
    cell::Cell,
    path::Path,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        System::Threading::*,
        UI::{HiDpi::GetDpiForSystem, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::PWSTR,
};

static AUTOMATIC: AtomicBool = AtomicBool::new(true);
static SHAKE: AtomicU8 = AtomicU8::new(0);

pub(super) const TIMER: usize = 1;
const POLL_MS: u32 = 15;
const RELEASE_TICKS: u32 = 4;
const OLE_CAPTURE_CLASS: &str = "CLIPBRDWNDCLASS";
const SHAKE_WINDOW: Duration = Duration::from_millis(800);
const ESCAPE_LPARAM: isize = 1 | (1 << 16);

impl ShakeLevel {
    fn span(self) -> i32 {
        match self {
            ShakeLevel::High => 15,
            _ => 30,
        }
    }

    fn turns(self) -> u32 {
        match self {
            ShakeLevel::High => 2,
            _ => 3,
        }
    }
}

pub fn set_automatic(enabled: bool) {
    AUTOMATIC.store(enabled, Ordering::Relaxed);
}

pub fn set_shake(level: ShakeLevel) {
    SHAKE.store(level as u8, Ordering::Relaxed);
}

fn shake_level() -> ShakeLevel {
    ShakeLevel::ALL[SHAKE.load(Ordering::Relaxed) as usize % ShakeLevel::ALL.len()]
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Shake {
    anchor: i32,
    direction: i32,
    turns: u32,
    needed: u32,
    since: Option<Instant>,
}

impl Shake {
    fn start(x: i32, needed: u32) -> Self {
        Self { anchor: x, needed, ..Self::default() }
    }

    fn feed(&mut self, x: i32, span: i32, now: Instant) -> bool {
        let delta = x - self.anchor;
        if delta.abs() < span {
            return false;
        }
        let direction = delta.signum();
        self.anchor = x;
        if self.direction != 0 && direction != self.direction {
            match self.since {
                Some(start) if now - start <= SHAKE_WINDOW => self.turns += 1,
                _ => {
                    self.since = Some(now);
                    self.turns = 1;
                }
            }
        }
        self.direction = direction;
        self.turns >= self.needed
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    thread: u32,
    source: Option<u32>,
    shown: bool,
    shake: Shake,
    released: u32,
}

thread_local! {
    static PRESS: Cell<Option<Press>> = const { Cell::new(None) };
    static SWALLOW_UP: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn unseen() -> bool {
    !exporting() && PRESS.get().is_some_and(|press| press.source.is_some() && !press.shown)
}

pub(super) fn mark_seen() {
    if let Some(mut press) = PRESS.get() {
        press.shown = true;
        PRESS.set(Some(press));
    }
}

pub fn buttons_down() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as _) < 0 || GetAsyncKeyState(VK_RBUTTON.0 as _) < 0 }
}

pub(super) fn class_name(window: HWND) -> String {
    let mut class = [0u16; 32];
    let length = unsafe { GetClassNameW(window, &mut class) } as usize;
    String::from_utf16_lossy(&class[..length.min(class.len())])
}

fn ole_capture(thread: u32) -> Option<HWND> {
    let mut info = GUITHREADINFO { cbSize: size_of::<GUITHREADINFO>() as u32, ..Default::default() };
    unsafe { GetGUIThreadInfo(thread, &mut info) }.ok()?;
    let capture = info.hwndCapture;
    (!capture.is_invalid() && class_name(capture) == OLE_CAPTURE_CLASS).then_some(capture)
}

fn ole_drag_in(thread: u32) -> Option<u32> {
    let capture = ole_capture(thread)?;
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(capture, Some(&mut process)) };
    (process != unsafe { GetCurrentProcessId() }).then_some(process)
}

fn external_drag(thread: u32) -> Option<u32> {
    ole_drag_in(thread).or_else(|| ole_drag_in(0))
}

fn abort_drag(thread: u32) -> bool {
    let own = unsafe { GetWindowThreadProcessId(window::panel(), None) };
    let Some(capture) = [thread, 0, own].into_iter().find_map(ole_capture) else {
        return false;
    };
    unsafe { PostMessageW(Some(capture), WM_KEYDOWN, WPARAM(VK_ESCAPE.0 as usize), LPARAM(ESCAPE_LPARAM)) }.is_ok()
}

pub(super) fn process_name(process: u32) -> String {
    let name = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process).ok().and_then(|handle| {
            let mut buffer = [0u16; MAX_PATH as usize];
            let mut length = buffer.len() as u32;
            let queried =
                QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length);
            let _ = CloseHandle(handle);
            queried.ok()?;
            let path = String::from_utf16_lossy(&buffer[..(length as usize).min(buffer.len())]);
            Path::new(&path).file_name().map(|name| name.to_string_lossy().into_owned())
        })
    };
    name.unwrap_or_else(|| format!("pid {process}"))
}

fn cursor_x() -> i32 {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    point.x
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Signal {
    Start,
    End,
}

#[derive(Clone, Copy)]
struct Poll {
    held: bool,
    exporting: bool,
    automatic: bool,
    shake: ShakeLevel,
    span: i32,
    now: Instant,
}

fn reveal(press: &mut Press) -> Option<Signal> {
    press.shown = true;
    Some(Signal::Start)
}

fn finish(press: Press, reason: &str) -> Option<Signal> {
    press.source.map(|_| {
        log::debug!("External drag ended: {reason}");
        Signal::End
    })
}

fn step(
    mut press: Press,
    poll: Poll,
    probe: impl FnOnce() -> Option<u32>,
    cursor: impl Fn() -> i32,
) -> (Option<Press>, Option<Signal>) {
    press.released = if poll.held { 0 } else { press.released + 1 };
    let released = press.released >= RELEASE_TICKS;
    let source = if released || poll.exporting { None } else { probe() };
    let signal = match (press.source, source) {
        (None, Some(process)) => {
            log::debug!("External drag detected: source {}", process_name(process));
            if !poll.automatic {
                log::debug!("Auto-show is off, the window stays as it is");
                None
            } else if poll.shake != ShakeLevel::Off {
                log::debug!("Waiting for a shake to show the window");
                press.shake = Shake::start(cursor(), poll.shake.turns());
                None
            } else {
                reveal(&mut press)
            }
        }
        (Some(_), Some(_)) if poll.automatic && poll.shake != ShakeLevel::Off && !press.shown => {
            if press.shake.feed(cursor(), poll.span, poll.now) {
                log::debug!("Shake detected, showing the window");
                reveal(&mut press)
            } else {
                None
            }
        }
        (Some(_), None) if poll.held => {
            finish(press, "drag cancelled while the button is still held (Esc or source cancelled)")
        }
        (Some(_), None) => finish(press, "mouse button released (dropped)"),
        _ => None,
    };
    if source.is_none() {
        press.shown = false;
    }
    press.source = source;
    ((!released).then_some(press), signal)
}

fn cancels(press: Option<Press>, exporting: bool) -> bool {
    exporting || press.is_some_and(|press| press.source.is_some())
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RightPress {
    Pass,
    Cancel,
    Swallow,
}

fn right_press(dragging: bool, shown: bool, left_held: bool) -> RightPress {
    match (dragging, shown && left_held) {
        (false, _) => RightPress::Pass,
        (true, false) => RightPress::Cancel,
        (true, true) => RightPress::Swallow,
    }
}

fn right_down(data: &MSLLHOOKSTRUCT) -> bool {
    let dragging = cancels(PRESS.get(), exporting());
    let left_held = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as _) } < 0;
    match right_press(dragging, window::visible(), left_held) {
        RightPress::Pass => {
            press_at(data);
            false
        }
        RightPress::Cancel => {
            log::debug!("Right button pressed during a drag");
            send(Event::CancelDrag);
            false
        }
        RightPress::Swallow => {
            let thread = PRESS.get().map(|press| press.thread).unwrap_or_default();
            let aborted = abort_drag(thread);
            log::debug!("Right button pressed during a drag, the drag is aborted: {aborted}");
            send(Event::CancelDrag);
            SWALLOW_UP.set(aborted);
            aborted
        }
    }
}

fn press_at(data: &MSLLHOOKSTRUCT) {
    if window::visible() && !window::contains(data.pt) {
        send(Event::PressOutside);
    }
    press(thread_at(data.pt));
}

pub(super) fn poll_press(point: POINT) {
    let thread = thread_at(point);
    if PRESS.get().is_none() && external_drag(thread).is_some() {
        log::debug!("A drag is running in a window the mouse hook doesn't see");
        press(thread);
    }
}

fn thread_at(point: POINT) -> u32 {
    unsafe { GetWindowThreadProcessId(WindowFromPoint(point), None) }
}

fn press(thread: u32) {
    let previous = PRESS.get();
    PRESS.set(Some(Press {
        thread,
        source: previous.and_then(|press| press.source),
        shown: previous.is_some_and(|press| press.shown),
        shake: previous.map(|press| press.shake).unwrap_or_default(),
        released: 0,
    }));
    unsafe {
        SetTimer(Some(relay::window()), TIMER, POLL_MS, None);
    }
}

fn stop_timer() {
    unsafe {
        let _ = KillTimer(Some(relay::window()), TIMER);
    }
}

pub(super) fn tick() {
    let Some(press) = PRESS.get() else {
        stop_timer();
        return;
    };
    let shake = shake_level();
    let source = external_drag(press.thread);
    let poll = Poll {
        held: buttons_down() || source.is_some(),
        exporting: exporting(),
        automatic: AUTOMATIC.load(Ordering::Relaxed),
        shake,
        span: shake.span() * unsafe { GetDpiForSystem() }.max(96) as i32 / 96,
        now: Instant::now(),
    };
    let (next, signal) = step(press, poll, || source, cursor_x);
    match signal {
        Some(Signal::Start) => send(Event::DragStart),
        Some(Signal::End) => send(Event::DragEnd),
        None => {}
    }
    PRESS.set(next);
    if next.is_none() {
        stop_timer();
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let swallow = guarded(false, || {
        if code < 0 {
            return false;
        }
        let Some(data) = (unsafe { (lp.0 as *const MSLLHOOKSTRUCT).as_ref() }) else {
            return false;
        };
        match wp.0 as u32 {
            WM_MOUSEMOVE => {
                corner::feed(data.pt);
                false
            }
            WM_RBUTTONDOWN => right_down(data),
            WM_RBUTTONUP => SWALLOW_UP.replace(false),
            WM_LBUTTONDOWN => {
                press_at(data);
                false
            }
            _ => false,
        }
    });
    if swallow { LRESULT(1) } else { unsafe { CallNextHookEx(None, code, wp, lp) } }
}

pub(super) fn install() -> Option<HHOOK> {
    let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) }.ok()?;
    match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(instance.into()), 0) } {
        Ok(hook) => {
            log::debug!("Mouse hook installed");
            Some(hook)
        }
        Err(error) => {
            log::error!("Couldn't install the mouse hook: {error}");
            None
        }
    }
}

pub(super) fn uninstall(hook: HHOOK) {
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
}

#[cfg(test)]
mod tests;
