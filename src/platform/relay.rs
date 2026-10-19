use super::{Event, drag, guarded, send};
use crate::i18n::{fill, tr};
use crate::keys::{Command, Shortcut};
use crate::profile;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{DataExchange::COPYDATASTRUCT, LibraryLoader::GetModuleHandleW, Threading::*},
        UI::{Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{HSTRING, PCWSTR, w},
};

static RELAY: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static TRAY_SHOWN: AtomicBool = AtomicBool::new(true);
static STOPPING: AtomicBool = AtomicBool::new(false);

const TRAY_MESSAGE: u32 = WM_APP + 1;
const REVEAL_MESSAGE: u32 = WM_APP + 2;
const BIND_MESSAGE: u32 = WM_APP + 3;
const CAPTURE_MESSAGE: u32 = WM_APP + 4;
const QUIT_MESSAGE: u32 = WM_APP + 5;
const TRAY_SHOW_MESSAGE: u32 = WM_APP + 6;
const BIND_TIMEOUT_MS: u32 = 1000;
const PATHS_DATA: usize = 0x5452_4458;
const FORWARD_ATTEMPTS: u32 = 50;
const FORWARD_RETRY: Duration = Duration::from_millis(100);
const FORWARD_TIMEOUT_MS: u32 = 5000;
const END_SESSION_WAIT: Duration = Duration::from_secs(5);
const END_SESSION_POLL: Duration = Duration::from_millis(20);
const PREVIOUS_EXIT_WAIT: Duration = Duration::from_secs(15);
const PREVIOUS_EXIT_POLL: Duration = Duration::from_millis(100);

thread_local! {
    static TRAY: RefCell<Option<NOTIFYICONDATAW>> = const { RefCell::new(None) };
    static HOTKEYS: RefCell<BTreeMap<Command, Shortcut>> = const { RefCell::new(BTreeMap::new()) };
    static KEYBOARD: Cell<Option<HHOOK>> = const { Cell::new(None) };
    static HELD: Cell<Shortcut> = Cell::new(Shortcut::default());
}

pub(super) fn window() -> HWND {
    HWND(RELAY.load(Ordering::Relaxed) as _)
}

pub fn single_instance(paths: &[PathBuf], after_previous: bool) -> bool {
    let deadline = Instant::now() + if after_previous { PREVIOUS_EXIT_WAIT } else { Duration::ZERO };
    loop {
        unsafe {
            let Ok(mutex) = CreateMutexW(None, false, &HSTRING::from(profile::current().mutex())) else {
                return true;
            };
            if GetLastError() != ERROR_ALREADY_EXISTS {
                return true;
            }
            if Instant::now() >= deadline {
                break;
            }
            let _ = CloseHandle(mutex);
        }
        std::thread::sleep(PREVIOUS_EXIT_POLL);
    }
    forward(paths);
    false
}

fn forward(paths: &[PathBuf]) {
    for _ in 0..FORWARD_ATTEMPTS {
        if let Ok(existing) = unsafe { FindWindowW(&HSTRING::from(profile::current().relay_class()), None) } {
            if paths.is_empty() {
                let _ = unsafe { PostMessageW(Some(existing), REVEAL_MESSAGE, WPARAM(0), LPARAM(0)) };
                return;
            }
            let data: Vec<u16> = paths.iter().flat_map(|p| p.as_os_str().encode_wide().chain([0])).collect();
            let copy =
                COPYDATASTRUCT { dwData: PATHS_DATA, cbData: (data.len() * 2) as u32, lpData: data.as_ptr() as _ };
            unsafe {
                SendMessageTimeoutW(
                    existing,
                    WM_COPYDATA,
                    WPARAM(0),
                    LPARAM(&copy as *const _ as isize),
                    SMTO_ABORTIFHUNG,
                    FORWARD_TIMEOUT_MS,
                    None,
                );
            }
            return;
        }
        std::thread::sleep(FORWARD_RETRY);
    }
}

fn received(lp: LPARAM) -> Option<Vec<PathBuf>> {
    let data = unsafe { (lp.0 as *const COPYDATASTRUCT).as_ref() }?;
    if data.dwData != PATHS_DATA || data.lpData.is_null() {
        return None;
    }
    let words = unsafe { std::slice::from_raw_parts(data.lpData as *const u16, data.cbData as usize / 2) };
    Some(words.split(|w| *w == 0).filter(|w| !w.is_empty()).map(|w| PathBuf::from(OsString::from_wide(w))).collect())
}

fn wide<const N: usize>(text: &str) -> [u16; N] {
    let mut slot = [0; N];
    let room = slot.len().saturating_sub(1);
    for (slot, ch) in slot[..room].iter_mut().zip(text.encode_utf16()) {
        *slot = ch;
    }
    slot
}

pub fn notify(text: &str) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window(),
        uID: 1,
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_NONE,
        ..Default::default()
    };
    data.szInfoTitle = wide(profile::current().name());
    data.szInfo = wide(text);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

pub fn start(hotkeys: Vec<(Command, Shortcut)>) {
    let started = std::thread::Builder::new().name("traydex-relay".into()).spawn(move || run(hotkeys));
    if let Err(error) = started {
        log::error!("Couldn't start the relay thread: {error}");
        send(Event::Failure(tr().hook_failed.into()));
    }
}

pub fn stop() {
    STOPPING.store(true, Ordering::Relaxed);
    unsafe {
        let _ = PostMessageW(Some(window()), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

pub fn set_tray(visible: bool) {
    TRAY_SHOWN.store(visible, Ordering::Relaxed);
    let relay = window();
    if !relay.is_invalid() {
        unsafe {
            let _ = PostMessageW(Some(relay), TRAY_SHOW_MESSAGE, WPARAM(0), LPARAM(0));
        }
    }
}

fn show_tray(tray: &NOTIFYICONDATAW) {
    let (action, verb) = if TRAY_SHOWN.load(Ordering::Relaxed) { (NIM_ADD, "add") } else { (NIM_DELETE, "remove") };
    if !unsafe { Shell_NotifyIconW(action, tray) }.as_bool() {
        log::debug!("Couldn't {verb} the tray icon");
    }
}

pub fn bind(command: Command, shortcut: Shortcut) -> bool {
    let mut result = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(
            window(),
            BIND_MESSAGE,
            WPARAM(command as usize),
            LPARAM(pack(shortcut)),
            SMTO_ABORTIFHUNG,
            BIND_TIMEOUT_MS,
            Some(&mut result as *mut usize),
        )
    };
    sent.0 != 0 && result == 1
}

pub fn capture(enabled: bool) {
    unsafe {
        let _ = PostMessageW(Some(window()), CAPTURE_MESSAGE, WPARAM(enabled as usize), LPARAM(0));
    }
}

fn pack(shortcut: Shortcut) -> isize {
    let flags = [shortcut.ctrl, shortcut.shift, shortcut.alt, shortcut.win];
    flags
        .into_iter()
        .enumerate()
        .fold(shortcut.key as isize, |packed, (bit, held)| packed | (held as isize) << (16 + bit))
}

fn unpack(packed: isize) -> Shortcut {
    let held = |bit: usize| packed & (1 << (16 + bit)) != 0;
    Shortcut { ctrl: held(0), shift: held(1), alt: held(2), win: held(3), key: packed as u16 }
}

fn hotkey_id(command: Command) -> i32 {
    command as i32 + 1
}

fn register(window: HWND, command: Command, shortcut: Shortcut) -> bool {
    let mut modifiers = MOD_NOREPEAT;
    for (held, flag) in
        [(shortcut.ctrl, MOD_CONTROL), (shortcut.shift, MOD_SHIFT), (shortcut.alt, MOD_ALT), (shortcut.win, MOD_WIN)]
    {
        if held {
            modifiers |= flag;
        }
    }
    match unsafe { RegisterHotKey(Some(window), hotkey_id(command), modifiers, shortcut.key as u32) } {
        Ok(()) => true,
        Err(error) => {
            log::warn!("Couldn't register {} for {command:?}: {error}", shortcut.text());
            false
        }
    }
}

fn rebind(window: HWND, command: Command, shortcut: Shortcut) -> bool {
    let previous = HOTKEYS.with_borrow(|hotkeys| hotkeys.get(&command).copied());
    if previous == Some(shortcut) {
        return true;
    }
    if previous.is_some() {
        unsafe {
            let _ = UnregisterHotKey(Some(window), hotkey_id(command));
        }
    }
    let bound = register(window, command, shortcut);
    let active = if bound { Some(shortcut) } else { previous.filter(|previous| register(window, command, *previous)) };
    HOTKEYS.with_borrow_mut(|hotkeys| match active {
        Some(active) => hotkeys.insert(command, active),
        None => hotkeys.remove(&command),
    });
    if command == Command::Toggle {
        refresh_tip();
    }
    bound
}

fn unregister_all(window: HWND) {
    for command in HOTKEYS.take().into_keys() {
        unsafe {
            let _ = UnregisterHotKey(Some(window), hotkey_id(command));
        }
    }
}

fn tip() -> String {
    let name = profile::current().name();
    match HOTKEYS.with_borrow(|hotkeys| hotkeys.get(&Command::Toggle).copied()) {
        Some(shortcut) => format!("{name} · {}", shortcut.text()),
        None => name.into(),
    }
}

fn refresh_tip() {
    TRAY.with_borrow_mut(|tray| {
        if let Some(tray) = tray {
            tray.szTip = wide(&tip());
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, tray);
            }
        }
    });
}

unsafe extern "system" fn keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let swallowed = guarded(false, || {
        if code < 0 {
            return false;
        }
        let Some(data) = (unsafe { (lp.0 as *const KBDLLHOOKSTRUCT).as_ref() }) else {
            return false;
        };
        let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let key = VIRTUAL_KEY(data.vkCode as u16);
        let mut held = HELD.get();
        let flag = match key {
            VK_CONTROL | VK_LCONTROL | VK_RCONTROL => Some(&mut held.ctrl),
            VK_SHIFT | VK_LSHIFT | VK_RSHIFT => Some(&mut held.shift),
            VK_MENU | VK_LMENU | VK_RMENU => Some(&mut held.alt),
            VK_LWIN | VK_RWIN => Some(&mut held.win),
            _ => None,
        };
        match flag {
            Some(flag) if *flag != down => {
                *flag = down;
                HELD.set(held);
                send(Event::Recording(held));
            }
            Some(_) => {}
            None if down => send(Event::Recorded(Shortcut { key: key.0, ..held })),
            None => {}
        }
        down
    });
    if swallowed {
        return LRESULT(1);
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn set_capture(enabled: bool) {
    match (enabled, KEYBOARD.get()) {
        (true, None) => {
            HELD.set(Shortcut::default());
            let instance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
            match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(instance.into()), 0) } {
                Ok(hook) => {
                    log::debug!("Keyboard capture for a shortcut started, all keys are swallowed");
                    KEYBOARD.set(Some(hook));
                }
                Err(error) => {
                    log::error!("Couldn't install the keyboard hook: {error}");
                    send(Event::Failure(tr().hook_failed.into()));
                }
            }
        }
        (false, Some(hook)) => {
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
            log::debug!("Keyboard capture for a shortcut ended");
            KEYBOARD.set(None);
        }
        _ => {}
    }
}

fn tray_menu(window: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, 1, &HSTRING::from(tr().tray_open));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, 2, &HSTRING::from(tr().tray_quit));
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let _ = SetForegroundWindow(window);
        let choice = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            Some(0),
            window,
            None,
        );
        match choice.0 {
            1 => send(Event::Reveal),
            2 => send(Event::Quit),
            _ => {}
        }
        let _ = DestroyMenu(menu);
    }
}

fn dispatch(window: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_TIMER if wp.0 == drag::TIMER => drag::tick(),
        WM_HOTKEY => {
            if let Some(command) = Command::ALL.into_iter().find(|command| hotkey_id(*command) == wp.0 as i32) {
                send(Event::Hotkey(command));
            }
        }
        BIND_MESSAGE => {
            let bound = Command::ALL
                .into_iter()
                .find(|command| *command as usize == wp.0)
                .is_some_and(|command| rebind(window, command, unpack(lp.0)));
            return Some(LRESULT(bound as isize));
        }
        CAPTURE_MESSAGE => set_capture(wp.0 != 0),
        TRAY_SHOW_MESSAGE => TRAY.with_borrow(|tray| tray.as_ref().map(show_tray)).unwrap_or_default(),
        WM_COPYDATA => {
            if let Some(paths) = received(lp) {
                send(Event::Receive(paths));
            }
            return Some(LRESULT(1));
        }
        REVEAL_MESSAGE => send(Event::Reveal),
        QUIT_MESSAGE if profile::current().testing() => send(Event::Quit),
        TRAY_MESSAGE => match lp.0 as u32 & 0xffff {
            WM_LBUTTONUP => send(Event::Toggle),
            WM_RBUTTONUP => tray_menu(window),
            _ => {}
        },
        WM_QUERYENDSESSION => return Some(LRESULT(1)),
        WM_ENDSESSION if wp.0 != 0 => {
            log::info!("Windows is ending the session, quitting");
            quit_and_wait();
        }
        WM_CLOSE if STOPPING.load(Ordering::Relaxed) => unsafe {
            let _ = DestroyWindow(window);
        },
        WM_CLOSE => {
            log::info!("Asked to close from outside, quitting");
            send(Event::Quit);
        }
        WM_DESTROY => unsafe { PostQuitMessage(0) },
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            TRAY.with_borrow(|tray| tray.as_ref().map(show_tray)).unwrap_or_default()
        }
        _ => return None,
    }
    Some(LRESULT(0))
}

fn quit_and_wait() {
    send(Event::Quit);
    let deadline = Instant::now() + END_SESSION_WAIT;
    while !STOPPING.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(END_SESSION_POLL);
    }
}

unsafe extern "system" fn relay_proc(window: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    guarded(None, || dispatch(window, msg, wp, lp)).unwrap_or_else(|| unsafe { DefWindowProcW(window, msg, wp, lp) })
}

fn create_window(instance: HINSTANCE) -> Option<HWND> {
    let profile = profile::current();
    let name = HSTRING::from(profile.relay_class());
    let class = WNDCLASSW {
        lpfnWndProc: Some(relay_proc),
        hInstance: instance,
        lpszClassName: PCWSTR(name.as_ptr()),
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            &name,
            &HSTRING::from(profile.name()),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
        .inspect_err(|error| log::error!("Couldn't create the relay window: {error}"))
        .ok()
    }
}

fn add_tray(window: HWND, instance: HINSTANCE) -> NOTIFYICONDATAW {
    let icon = unsafe {
        LoadIconW(Some(instance), PCWSTR(1usize as _)).or_else(|_| LoadIconW(None, IDI_APPLICATION)).unwrap_or_default()
    };
    let mut tray = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY_MESSAGE,
        hIcon: icon,
        ..Default::default()
    };
    tray.szTip = wide(&tip());
    unsafe {
        if TRAY_SHOWN.load(Ordering::Relaxed) && !Shell_NotifyIconW(NIM_ADD, &tray).as_bool() {
            log::warn!("Couldn't add the tray icon, will retry when the taskbar restarts");
        }
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
    }
    TRAY.set(Some(tray));
    tray
}

fn run(hotkeys: Vec<(Command, Shortcut)>) {
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }.unwrap_or_default().into();
    let Some(window) = create_window(instance) else {
        send(Event::Failure(tr().hook_failed.into()));
        return;
    };
    RELAY.store(window.0 as isize, Ordering::Relaxed);
    for (command, shortcut) in hotkeys {
        if !rebind(window, command, shortcut) {
            send(Event::Failure(fill(tr().hotkey_busy, shortcut.text())));
        }
    }
    let tray = add_tray(window, instance);
    let hook = drag::install();
    if hook.is_none() {
        send(Event::Failure(tr().hook_failed.into()));
    }
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = Shell_NotifyIconW(NIM_DELETE, &tray);
    }
    set_capture(false);
    unregister_all(window);
    if let Some(hook) = hook {
        drag::uninstall(hook);
    }
    log::debug!("Relay thread stopped");
}

#[cfg(test)]
mod tests;
