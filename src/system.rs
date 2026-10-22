use crate::keys::{Command, Shortcut};
use crate::store::{Corner, Item, Payload, ShakeLevel, Storage};
use anyhow::Result;
use image::RgbaImage;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

pub enum Event {
    DragStart,
    DragToCorner,
    DragEnd,
    CancelDrag,
    Hover(bool),
    Offer(Offer),
    Pointer(f32, f32),
    Drop(Payload),
    DropBack,
    ExportEnd,
    Receive(Vec<PathBuf>),
    Placed { x: i32, y: i32, width: f32, height: f32 },
    ResetSize,
    FitSize,
    Toggle,
    Corner,
    PressOutside,
    Hotkey(Command),
    Recording(Shortcut),
    Recorded(Shortcut),
    Reveal,
    Escape,
    Quit,
    Failure(String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Offer {
    Files(Vec<Offered>),
    Text,
    Image,
    Unknown,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Offered {
    pub path: PathBuf,
    pub folder: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Frame {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Handler(pub u128);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppData {
    Roaming,
    Local,
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Caption {
    pub height: f32,
    pub leading: (f32, f32),
    pub trailing: f32,
}

#[derive(Clone, Copy, Default)]
pub struct Stage {
    pub fraction: Option<f32>,
}

#[derive(Clone, Default)]
pub struct Progress {
    stage: Arc<Mutex<Stage>>,
    cancelled: Arc<AtomicBool>,
    expired: Arc<AtomicBool>,
}

impl Progress {
    pub fn get(&self) -> Stage {
        *self.stage.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn update(&self, change: impl FnOnce(&mut Stage)) {
        change(&mut self.stage.lock().unwrap_or_else(PoisonError::into_inner));
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn expire(&self) {
        self.expired.store(true, Ordering::Relaxed);
        self.cancel();
    }

    pub fn expired(&self) -> bool {
        self.expired.load(Ordering::Relaxed)
    }

    pub fn stopped(&self) -> bool {
        self.cancelled() && !self.expired()
    }
}

pub struct Server {
    pub url: String,
    pub files: Vec<PathBuf>,
    _running: Option<Box<dyn Send>>,
}

impl Server {
    pub fn new(url: String, files: Vec<PathBuf>, running: impl Send + 'static) -> Self {
        Self { url, files, _running: Some(Box::new(running)) }
    }

    #[cfg(test)]
    pub fn detached(url: String, files: Vec<PathBuf>) -> Self {
        Self { url, files, _running: None }
    }
}

pub trait Embedded {
    fn place(&self, frame: Frame);
}

pub trait Desktop {
    fn layout_char(&self, key: u16) -> Option<char>;

    fn visible(&self) -> bool;
    fn show(&self, activate: bool);
    fn hide(&self);
    fn focus(&self);
    fn pointer_inside(&self) -> bool;
    fn buttons_down(&self) -> bool;
    fn exporting(&self) -> bool;
    fn prepare(&self, width: f32, height: f32);
    fn place(&self, position: Option<(i32, i32)>);
    fn place_near_cursor(&self);
    fn resize(&self, width: f32, height: f32);
    fn reset_position(&self);
    fn set_min_size(&self, width: f32, height: f32);
    fn set_caption(&self, caption: Caption);

    fn set_automatic(&self, enabled: bool);
    fn set_shake(&self, level: ShakeLevel);
    fn set_corner(&self, corner: Option<Corner>);
    fn set_corner_drag(&self, enabled: bool);
    fn set_tray(&self, visible: bool);
    fn set_reject(&self, reject: bool);
    fn bind(&self, command: Command, shortcut: Shortcut) -> bool;
    fn capture(&self, enabled: bool);
    fn stop_watch(&self);
    fn notify(&self, text: &str);
    fn set_autostart(&self, enabled: bool) -> Result<()>;
    fn set_context_menu(&self, enabled: bool, label: &str) -> Result<()>;

    fn open(&self, item: &Item);
    fn reveal(&self, item: &Item);
    fn copy(&self, items: Vec<Item>) -> Result<()>;
    fn paste(&self, storage: &Storage) -> Result<Payload>;
    fn drag(&self, items: Vec<Item>) -> Result<()>;
    fn flush_clipboard(&self);
    fn embed(
        &self,
        handler: Handler,
        path: &Path,
        frame: Frame,
        background: u32,
        text: u32,
    ) -> Result<Box<dyn Embedded>>;
}

pub trait Services: Send + Sync {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>);

    fn shell_open(&self, target: &str);
    fn pick(&self, title: &str, folders: bool) -> Result<Option<PathBuf>>;
    fn pick_files(&self, title: &str) -> Result<Vec<PathBuf>>;
    fn display_name(&self, path: &Path) -> String;
    fn desktop_folder(&self) -> Option<PathBuf>;
    fn transfer(&self, files: &[PathBuf], folder: &Path, copy: bool) -> Result<()>;
    fn recycle(&self, paths: &[PathBuf]) -> Result<()>;

    fn thumbnail(&self, path: &Path, size: i32) -> Option<(u32, u32, Vec<u8>)>;
    fn preview_handler(&self, path: &Path) -> Option<Handler>;
    fn type_name(&self, path: &Path) -> Option<String>;

    fn launch(&self, command: &mut process::Command) -> io::Result<()>;
    fn output(&self, command: &mut process::Command, input: Option<Vec<u8>>, progress: &Progress)
    -> io::Result<Output>;
    fn stream(
        &self,
        command: &mut process::Command,
        input: Option<Vec<u8>>,
        progress: &Progress,
        line: &mut dyn FnMut(&str),
    ) -> io::Result<Output>;
    fn download(&self, url: &str, target: &Path, progress: &Progress) -> Result<()>;
    fn serve(&self, files: Vec<PathBuf>) -> Result<Server>;
    fn search_path(&self) -> Option<OsString>;
    fn temp_dir(&self) -> PathBuf;
    fn app_data(&self, place: AppData) -> Option<PathBuf>;

    fn recognize(&self, image: &RgbaImage, language: &str) -> Result<String>;
    fn ocr_languages(&self) -> Vec<(String, String)>;
    fn render_pdf(&self, path: &Path, each: &mut (dyn FnMut(RgbaImage) -> Result<()> + Send)) -> Result<()>;
    fn font(&self) -> Option<PathBuf>;
}
