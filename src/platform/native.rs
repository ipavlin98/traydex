use super::Host;
use crate::keys::{Command, Shortcut};
use crate::os;
use crate::store::{Corner, Item, Payload, ShakeLevel, Storage};
use crate::system::{AppData, Caption, Desktop, Embedded, Frame, Handler, Progress, Server, Services};
use anyhow::Result;
use image::RgbaImage;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Output};
use windows::Win32::Foundation::RECT;
use windows::core::GUID;

pub struct Native;

pub(super) fn rect(frame: Frame) -> RECT {
    RECT { left: frame.left, top: frame.top, right: frame.right, bottom: frame.bottom }
}

impl Embedded for Host {
    fn place(&self, frame: Frame) {
        Host::place(self, rect(frame));
    }
}

impl Desktop for Native {
    fn layout_char(&self, key: u16) -> Option<char> {
        os::layout_char(key)
    }

    fn visible(&self) -> bool {
        super::visible()
    }

    fn show(&self, activate: bool) {
        super::show(activate);
    }

    fn hide(&self) {
        super::hide();
    }

    fn focus(&self) {
        super::focus();
    }

    fn pointer_inside(&self) -> bool {
        super::pointer_inside()
    }

    fn buttons_down(&self) -> bool {
        super::buttons_down()
    }

    fn exporting(&self) -> bool {
        super::exporting()
    }

    fn prepare(&self, width: f32, height: f32) {
        super::prepare(width, height);
    }

    fn place(&self, position: Option<(i32, i32)>) {
        super::place(position);
    }

    fn place_near_cursor(&self) {
        super::place_near_cursor();
    }

    fn resize(&self, width: f32, height: f32) {
        super::resize(width, height);
    }

    fn reset_position(&self) {
        super::reset_position();
    }

    fn set_min_size(&self, width: f32, height: f32) {
        super::set_min_size(width, height);
    }

    fn set_caption(&self, caption: Caption) {
        super::set_caption(caption);
    }

    fn set_automatic(&self, enabled: bool) {
        super::set_automatic(enabled);
    }

    fn set_shake(&self, level: ShakeLevel) {
        super::set_shake(level);
    }

    fn set_corner(&self, corner: Option<Corner>) {
        super::set_corner(corner);
    }

    fn set_corner_drag(&self, enabled: bool) {
        super::set_corner_drag(enabled);
    }

    fn set_tray(&self, visible: bool) {
        super::set_tray(visible);
    }

    fn set_reject(&self, reject: bool) {
        super::set_reject(reject);
    }

    fn bind(&self, command: Command, shortcut: Shortcut) -> bool {
        super::bind(command, shortcut)
    }

    fn capture(&self, enabled: bool) {
        super::capture(enabled);
    }

    fn stop_watch(&self) {
        super::stop_watch();
    }

    fn notify(&self, text: &str) {
        super::notify(text);
    }

    fn set_autostart(&self, enabled: bool) -> Result<()> {
        super::set_autostart(enabled)
    }

    fn set_context_menu(&self, enabled: bool, label: &str) -> Result<()> {
        super::set_context_menu(enabled, label)
    }

    fn open(&self, item: &Item) {
        super::open(item);
    }

    fn reveal(&self, item: &Item) {
        super::reveal(item);
    }

    fn copy(&self, items: Vec<Item>) -> Result<()> {
        super::copy(items)
    }

    fn paste(&self, storage: &Storage) -> Result<Payload> {
        super::paste(storage)
    }

    fn drag(&self, items: Vec<Item>) -> Result<()> {
        super::drag(items)
    }

    fn flush_clipboard(&self) {
        super::flush_clipboard();
    }

    fn embed(
        &self,
        handler: Handler,
        path: &Path,
        frame: Frame,
        background: u32,
        text: u32,
    ) -> Result<Box<dyn Embedded>> {
        Ok(Box::new(Host::open(GUID::from_u128(handler.0), path, rect(frame), background, text)?))
    }
}

impl Services for Native {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) {
        super::spawn(job);
    }

    fn shell_open(&self, target: &str) {
        super::shell_open(target);
    }

    fn pick(&self, title: &str, folders: bool) -> Result<Option<PathBuf>> {
        super::pick(title, folders)
    }

    fn pick_files(&self, title: &str) -> Result<Vec<PathBuf>> {
        super::pick_files(title)
    }

    fn display_name(&self, path: &Path) -> String {
        super::display_name(path)
    }

    fn desktop_folder(&self) -> Option<PathBuf> {
        super::desktop()
    }

    fn transfer(&self, files: &[PathBuf], folder: &Path, copy: bool) -> Result<()> {
        super::transfer(files, folder, copy)
    }

    fn thumbnail(&self, path: &Path, size: i32) -> Option<(u32, u32, Vec<u8>)> {
        super::thumbnail(path, size)
    }

    fn preview_handler(&self, path: &Path) -> Option<Handler> {
        super::preview_handler(path).map(|clsid| Handler(clsid.to_u128()))
    }

    fn type_name(&self, path: &Path) -> Option<String> {
        super::type_name(path)
    }

    fn launch(&self, command: &mut process::Command) -> io::Result<()> {
        command.spawn().map(drop)
    }

    fn output(
        &self,
        command: &mut process::Command,
        input: Option<Vec<u8>>,
        progress: &Progress,
    ) -> io::Result<Output> {
        super::child::output(command, input, progress)
    }

    fn stream(
        &self,
        command: &mut process::Command,
        input: Option<Vec<u8>>,
        progress: &Progress,
        line: &mut dyn FnMut(&str),
    ) -> io::Result<Output> {
        super::child::stream(command, input, progress, line)
    }

    fn download(&self, url: &str, target: &Path, progress: &Progress) -> Result<()> {
        super::download(url, target, progress)
    }

    fn serve(&self, files: Vec<PathBuf>) -> Result<Server> {
        crate::actions::serve(files)
    }

    fn search_path(&self) -> Option<OsString> {
        std::env::var_os("PATH")
    }

    fn temp_dir(&self) -> PathBuf {
        crate::profile::current().temp_dir()
    }

    fn app_data(&self, place: AppData) -> Option<PathBuf> {
        let name = match place {
            AppData::Roaming => "APPDATA",
            AppData::Local => "LOCALAPPDATA",
        };
        std::env::var_os(name).map(PathBuf::from)
    }

    fn recognize(&self, image: &RgbaImage, language: &str) -> Result<String> {
        super::recognize(image, language)
    }

    fn ocr_languages(&self) -> Vec<(String, String)> {
        super::ocr_languages()
    }

    fn render_pdf(&self, path: &Path, each: &mut (dyn FnMut(RgbaImage) -> Result<()> + Send)) -> Result<()> {
        super::render_pdf(path, each)
    }

    fn font(&self) -> Option<PathBuf> {
        super::font()
    }
}
