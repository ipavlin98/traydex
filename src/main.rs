#![windows_subsystem = "windows"]

mod actions;
mod design;
mod diagnostics;
mod geometry;
#[cfg(test)]
mod gpui_tests;
mod guard;
mod i18n;
mod icons;
mod input;
mod keys;
mod logging;
mod os;
mod platform;
mod preview;
mod profile;
mod search;
mod shelf;
mod store;
mod system;
#[cfg(test)]
mod testing;
mod textarea;
mod theme;
mod update;
mod view;

use futures::channel::mpsc;
use gpui::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use shelf::{Launch, Shelf};
use std::path::PathBuf;
use std::sync::Arc;
use windows::Win32::Foundation::HWND;

pub(crate) const PRODUCT: &str = concat!("Traydex ", env!("CARGO_PKG_VERSION"));

fn native_handle(window: &Window) -> Option<HWND> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(raw) => Some(HWND(raw.hwnd.get() as _)),
        _ => None,
    }
}

fn main() {
    match profile::Profile::from_args(std::env::args_os().skip(1), platform::SEND_FLAG) {
        Ok(profile) if profile.testing() => profile::init(profile),
        Ok(profile) => profile::init(
            std::env::current_exe().ok().and_then(|exe| profile::Profile::portable_beside(&exe)).unwrap_or(profile),
        ),
        Err(_) => std::process::exit(2),
    }
    let sent: Vec<PathBuf> =
        std::env::args_os().skip_while(|arg| arg != platform::SEND_FLAG).skip(1).map(PathBuf::from).collect();
    let flag = |name: &str| std::env::args().any(|arg| arg == name);
    if !platform::single_instance(&sent, flag(update::WAIT_FLAG)) {
        return;
    }
    let _logger = logging::start();
    if let Ok(exe) = std::env::current_exe() {
        update::sweep(&profile::current().temp_dir(), &exe);
    }
    let launch = if flag(platform::AUTOSTART_FLAG) {
        Launch::Hidden
    } else if profile::current().testing() && flag(profile::INACTIVE_FLAG) {
        Launch::Inactive
    } else {
        Launch::Active
    };
    let hidden = launch != Launch::Active;
    let (tx, rx) = mpsc::unbounded();
    platform::connect(tx);
    if !sent.is_empty() {
        platform::send(system::Event::Receive(sent));
    }
    Application::new().with_assets(icons::Assets).run(move |cx: &mut App| {
        let saved = store::Store::load();
        i18n::set(saved.settings().language.unwrap_or_else(i18n::Language::system));
        let (width, height) = saved.settings().window_size();
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                kind: WindowKind::PopUp,
                is_resizable: false,
                is_minimizable: false,
                window_min_size: Some(size(px(store::MIN_SIZE.0), px(store::MIN_SIZE.1))),
                window_background: WindowBackgroundAppearance::Opaque,
                focus: !hidden,
                show: false,
                ..Default::default()
            },
            move |window, cx| {
                if let Some(hwnd) = native_handle(window)
                    && let Err(error) = platform::attach(hwnd, hidden, saved.storage().clone())
                {
                    log::error!("Couldn't attach to the window: {error:#}");
                }
                platform::start_watch(saved.settings().shortcuts.system());
                window.on_window_should_close(cx, |_, _| {
                    if platform::visible() {
                        platform::send(system::Event::Toggle);
                    }
                    false
                });
                let native = Arc::new(platform::Native);
                cx.new(|cx| Shelf::new(saved, rx, launch, native.clone(), native, window, cx))
            },
        );
        if let Err(error) = result {
            log::error!("Couldn't open the window: {error:#}");
            cx.quit();
            return;
        }
        if !hidden {
            cx.activate(true);
        }
    });
    platform::stop_watch();
}
