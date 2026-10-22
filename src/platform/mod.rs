mod child;
mod corner;
mod download;
mod drag;
mod media;
mod native;
mod ole;
mod preview;
mod registry;
mod relay;
mod shell;
mod window;

pub use corner::{set_corner, set_corner_drag};
pub use download::download;
pub use drag::{buttons_down, set_automatic, set_shake};
pub use media::{font, ocr_languages, recognize, render_pdf};
pub use native::Native;
pub use ole::{copy, drag, exporting, flush_clipboard, paste, set_reject};
pub use preview::{Host, focus, preview_handler, thumbnail, type_name};
pub use registry::{AUTOSTART_FLAG, SEND_FLAG, set_autostart, set_context_menu};
pub use relay::{bind, capture, notify, set_tray, single_instance, start as start_watch, stop as stop_watch};
pub use shell::{desktop, display_name, open, pick, pick_files, recycle, reveal, shell_open, transfer};
pub use window::{
    hide, place, place_near_cursor, pointer_inside, prepare, reset_position, resize, set_caption, set_min_size, show,
    visible,
};

use crate::{
    guard,
    store::{Payload, Storage},
    system::Event,
};
use futures::channel::mpsc::UnboundedSender;
use std::sync::OnceLock;
use windows::Win32::{
    Foundation::HWND,
    System::Ole::{OleInitialize, OleUninitialize},
};

static EVENTS: OnceLock<UnboundedSender<Event>> = OnceLock::new();

pub fn connect(tx: UnboundedSender<Event>) {
    let _ = EVENTS.set(tx);
}

pub fn send(event: Event) {
    if let Some(tx) = EVENTS.get() {
        let _ = tx.unbounded_send(event);
    }
}

fn guarded<T>(fallback: T, job: impl FnOnce() -> T) -> T {
    guard::or_else(|| fallback, job)
}

pub fn attach(hwnd: HWND, hidden: bool, storage: Storage) -> anyhow::Result<()> {
    window::attach(hwnd, hidden);
    ole::register(hwnd, storage)
}

pub fn spawn(job: Box<dyn FnOnce() + Send>) {
    let started = std::thread::Builder::new().name("traydex-job".into()).spawn(move || {
        let initialized = unsafe { OleInitialize(None) }.is_ok();
        job();
        if initialized {
            unsafe { OleUninitialize() };
        }
    });
    if let Err(error) = started {
        log::error!("Couldn't start a background job: {error}");
    }
}
