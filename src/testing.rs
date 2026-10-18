use crate::i18n::{self, Language};
use crate::keys::{Command, Shortcut};
use crate::store::{Corner, Payload, ShakeLevel};
use crate::store::{Item, ItemId, Kind, Storage, Store};
use crate::system::{AppData, Caption, Desktop, Embedded, Frame, Handler, Progress, Server, Services};
use anyhow::{Result, anyhow};
use image::RgbaImage;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;
use std::os::windows::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{self, ExitStatus, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub const NARROW_SIZE: (f32, f32) = (384., 360.);

pub struct Sandbox {
    path: PathBuf,
}

impl Sandbox {
    pub fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "traydex-test-{}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            crate::store::id()
        ));
        std::fs::create_dir_all(&path).expect("sandbox");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        let mut path = self.path.clone();
        path.extend(name.as_ref().components());
        path
    }

    pub fn storage(&self) -> Storage {
        Storage::new(self.join("root"))
    }

    pub fn store(&self) -> Store {
        Store::open(self.storage(), &self.join("legacy"))
    }

    pub fn file(&self, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(&path, contents).expect("file");
        path
    }

    pub fn folder(&self, name: &str) -> PathBuf {
        let path = self.join(name);
        std::fs::create_dir_all(&path).expect("folder");
        path
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub struct Switched {
    previous: Language,
}

pub fn language(language: Language) -> Switched {
    let previous = i18n::current();
    i18n::set(language);
    Switched { previous }
}

impl Drop for Switched {
    fn drop(&mut self) {
        i18n::set(self.previous);
    }
}

pub fn item(kind: Kind, title: &str, path: PathBuf, text: Option<&str>) -> Item {
    Item {
        id: ItemId::new(),
        kind,
        title: title.into(),
        path,
        text: text.map(Into::into),
        managed: false,
        named: false,
        pinned: false,
    }
}

pub fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).expect("png");
    bytes.into_inner()
}

pub fn plant_tools(storage: &Storage) {
    let folder = storage.root().join("tools");
    std::fs::create_dir_all(&folder).expect("tools");
    for name in ["ffmpeg.exe", "yt-dlp.exe", "deno.exe"] {
        std::fs::write(folder.join(name), "MZ").expect("tool");
    }
}

pub fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).expect("zip"));
    for (name, data) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).expect("entry");
        writer.write_all(data).expect("write");
    }
    writer.finish().expect("finish");
}

pub fn plant_plugin(storage: &Storage, manifest: &str) {
    let parsed: serde_json::Value = serde_json::from_str(manifest).expect("manifest json");
    let dir = storage.root().join("actions").join("custom").join(parsed["id"].as_str().expect("id"));
    std::fs::create_dir_all(&dir).expect("plugin folder");
    std::fs::write(dir.join("action.json"), manifest).expect("manifest");
    std::fs::write(dir.join("main.ps1"), "exit 0").expect("entry");
}

pub fn video_info(title: &str) -> String {
    serde_json::json!({
        "_type": "video",
        "title": title,
        "extractor_key": "Youtube",
        "formats": [
            {"format_id": "140", "vcodec": "none", "height": null},
            {"format_id": "136", "vcodec": "avc1", "height": 720},
            {"format_id": "137", "vcodec": "avc1", "height": 1080},
        ],
        "subtitles": {"de": [], "en-US": []},
        "automatic_captions": {"ru": [], "en": [], "fr": []},
    })
    .to_string()
}

pub fn status(code: u32) -> ExitStatus {
    ExitStatus::from_raw(code)
}

pub fn output(code: u32, stdout: &str, stderr: &str) -> Output {
    Output { status: status(code), stdout: stdout.as_bytes().to_vec(), stderr: stderr.as_bytes().to_vec() }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub dir: Option<PathBuf>,
    pub input: Option<Vec<u8>>,
    pub envs: Vec<(String, String)>,
}

impl Invocation {
    fn of(command: &process::Command, input: Option<Vec<u8>>) -> Self {
        Self {
            program: command.get_program().into(),
            args: command.get_args().map(Into::into).collect(),
            dir: command.get_current_dir().map(Into::into),
            input,
            envs: command
                .get_envs()
                .filter_map(|(key, value)| {
                    Some((key.to_string_lossy().into_owned(), value?.to_string_lossy().into_owned()))
                })
                .collect(),
        }
    }

    pub fn args(&self) -> Vec<String> {
        self.args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    pub fn env(&self, key: &str) -> Option<&str> {
        self.envs.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Show(bool),
    Hide,
    Focus,
    Prepare(f32, f32),
    Place(Option<(i32, i32)>),
    PlaceNearCursor,
    Resize(f32, f32),
    ResetPosition,
    MinSize(f32, f32),
    Automatic(bool),
    Shake(ShakeLevel),
    Corner(Option<Corner>),
    CornerDrag(bool),
    Tray(bool),
    Reject(bool),
    Bind(Command, Shortcut),
    Capture(bool),
    StopWatch,
    Notify(String),
    Autostart(bool),
    ContextMenu(bool, String),
    Open(ItemId),
    Reveal(ItemId),
    ShellOpen(String),
    Pick(String, bool),
    PickFiles(String),
    Transfer(Vec<PathBuf>, PathBuf, bool),
    Copy(Vec<ItemId>),
    Paste,
    Drag(Vec<ItemId>),
    FlushClipboard,
    Embed(PathBuf),
    Launch(Invocation),
    Output(Invocation),
    Download(String, PathBuf),
    Serve(Vec<PathBuf>),
    RenderPdf(PathBuf),
}

type Downloader = Box<dyn FnMut(&str, &Path, &Progress) -> Result<()> + Send>;
type Streamer = Box<dyn FnMut(&Invocation) -> io::Result<Output> + Send>;

pub struct State {
    pub calls: Vec<Call>,
    pub visible: bool,
    pub inside: bool,
    pub buttons: bool,
    pub exporting: bool,
    pub jobs: VecDeque<Box<dyn FnOnce() + Send>>,
    pub autostart_error: Option<String>,
    pub menu_error: Option<String>,
    pub busy: Vec<Shortcut>,
    pub picked: VecDeque<Result<Option<PathBuf>>>,
    pub picked_files: VecDeque<Result<Vec<PathBuf>>>,
    pub desktop: Option<PathBuf>,
    pub pasted: VecDeque<Result<Payload>>,
    pub copy_error: Option<String>,
    pub drag_error: Option<String>,
    pub transfer_error: Option<String>,
    pub thumbnail: Option<(u32, u32, Vec<u8>)>,
    pub handler: Option<Handler>,
    pub type_name: Option<String>,
    pub embed_error: Option<String>,
    pub outputs: VecDeque<io::Result<Output>>,
    pub stop_on_output: bool,
    pub launch_error: Option<io::ErrorKind>,
    pub downloader: Option<Downloader>,
    pub streamer: Option<Streamer>,
    pub roaming: Option<PathBuf>,
    pub local: Option<PathBuf>,
    pub serve_error: Option<String>,
    pub search_path: Option<OsString>,
    pub temp: PathBuf,
    pub recognized: VecDeque<Result<String>>,
    pub scanned: Vec<RgbaImage>,
    pub read_in: Vec<String>,
    pub languages: Vec<(String, String)>,
    pub pages: Result<Vec<RgbaImage>, String>,
    pub font: Option<PathBuf>,
}

pub struct Fake {
    state: Mutex<State>,
}

struct Pane;

impl Embedded for Pane {
    fn place(&self, _: Frame) {}
}

impl Fake {
    pub fn new(sandbox: &Sandbox) -> Self {
        let temp = sandbox.join("temp");
        std::fs::create_dir_all(&temp).expect("temp");
        Self {
            state: Mutex::new(State {
                calls: Vec::new(),
                visible: false,
                inside: false,
                buttons: false,
                exporting: false,
                jobs: VecDeque::new(),
                autostart_error: None,
                menu_error: None,
                busy: Vec::new(),
                picked: VecDeque::new(),
                picked_files: VecDeque::new(),
                desktop: None,
                pasted: VecDeque::new(),
                copy_error: None,
                drag_error: None,
                transfer_error: None,
                thumbnail: None,
                handler: None,
                type_name: None,
                embed_error: None,
                outputs: VecDeque::new(),
                stop_on_output: false,
                launch_error: None,
                downloader: None,
                streamer: None,
                roaming: None,
                local: None,
                serve_error: None,
                search_path: Some(OsString::new()),
                temp,
                recognized: VecDeque::new(),
                scanned: Vec::new(),
                read_in: Vec::new(),
                languages: Vec::new(),
                pages: Ok(Vec::new()),
                font: None,
            }),
        }
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn calls(&self) -> Vec<Call> {
        self.state().calls.clone()
    }

    pub fn take_calls(&self) -> Vec<Call> {
        std::mem::take(&mut self.state().calls)
    }

    pub fn jobs(&self) -> usize {
        self.state().jobs.len()
    }

    pub fn run_jobs(&self) -> usize {
        let mut count = 0;
        loop {
            let Some(job) = self.state().jobs.pop_front() else {
                return count;
            };
            job();
            count += 1;
        }
    }

    pub fn drop_jobs(&self) {
        self.state().jobs.clear();
    }

    fn record(&self, call: Call) {
        self.state().calls.push(call);
    }

    fn failure(message: &Option<String>) -> Result<()> {
        match message {
            Some(message) => Err(anyhow!(message.clone())),
            None => Ok(()),
        }
    }
}

pub fn us_layout(key: u16) -> Option<char> {
    let oem = [
        (VK_OEM_1, ';'),
        (VK_OEM_PLUS, '='),
        (VK_OEM_COMMA, ','),
        (VK_OEM_MINUS, '-'),
        (VK_OEM_PERIOD, '.'),
        (VK_OEM_2, '/'),
        (VK_OEM_3, '`'),
        (VK_OEM_4, '['),
        (VK_OEM_5, '\\'),
        (VK_OEM_6, ']'),
        (VK_OEM_7, '\''),
    ];
    if (VK_A.0..=VK_Z.0).contains(&key) || (VK_0.0..=VK_9.0).contains(&key) {
        return char::from_u32(key as u32);
    }
    oem.iter().find(|(vk, _)| vk.0 == key).map(|(_, c)| *c)
}

impl Desktop for Fake {
    fn layout_char(&self, key: u16) -> Option<char> {
        us_layout(key)
    }

    fn visible(&self) -> bool {
        self.state().visible
    }

    fn show(&self, activate: bool) {
        let mut state = self.state();
        state.visible = true;
        state.calls.push(Call::Show(activate));
    }

    fn hide(&self) {
        let mut state = self.state();
        state.visible = false;
        state.calls.push(Call::Hide);
    }

    fn focus(&self) {
        self.record(Call::Focus);
    }

    fn pointer_inside(&self) -> bool {
        self.state().inside
    }

    fn buttons_down(&self) -> bool {
        self.state().buttons
    }

    fn exporting(&self) -> bool {
        self.state().exporting
    }

    fn prepare(&self, width: f32, height: f32) {
        self.record(Call::Prepare(width, height));
    }

    fn place(&self, position: Option<(i32, i32)>) {
        self.record(Call::Place(position));
    }

    fn place_near_cursor(&self) {
        self.record(Call::PlaceNearCursor);
    }

    fn resize(&self, width: f32, height: f32) {
        self.record(Call::Resize(width, height));
    }

    fn reset_position(&self) {
        self.record(Call::ResetPosition);
    }

    fn set_min_size(&self, width: f32, height: f32) {
        self.record(Call::MinSize(width, height));
    }

    fn set_automatic(&self, enabled: bool) {
        self.record(Call::Automatic(enabled));
    }

    fn set_shake(&self, level: ShakeLevel) {
        self.record(Call::Shake(level));
    }

    fn set_corner(&self, corner: Option<Corner>) {
        self.record(Call::Corner(corner));
    }

    fn set_corner_drag(&self, enabled: bool) {
        self.record(Call::CornerDrag(enabled));
    }

    fn set_tray(&self, visible: bool) {
        self.record(Call::Tray(visible));
    }

    fn set_reject(&self, reject: bool) {
        self.record(Call::Reject(reject));
    }

    fn bind(&self, command: Command, shortcut: Shortcut) -> bool {
        let mut state = self.state();
        state.calls.push(Call::Bind(command, shortcut));
        !state.busy.contains(&shortcut)
    }

    fn capture(&self, enabled: bool) {
        self.record(Call::Capture(enabled));
    }

    fn stop_watch(&self) {
        self.record(Call::StopWatch);
    }

    fn notify(&self, text: &str) {
        self.record(Call::Notify(text.into()));
    }

    fn set_autostart(&self, enabled: bool) -> Result<()> {
        let mut state = self.state();
        state.calls.push(Call::Autostart(enabled));
        Self::failure(&state.autostart_error)
    }

    fn set_context_menu(&self, enabled: bool, label: &str) -> Result<()> {
        let mut state = self.state();
        state.calls.push(Call::ContextMenu(enabled, label.into()));
        Self::failure(&state.menu_error)
    }

    fn open(&self, item: &Item) {
        self.record(Call::Open(item.id.clone()));
    }

    fn reveal(&self, item: &Item) {
        self.record(Call::Reveal(item.id.clone()));
    }

    fn copy(&self, items: Vec<Item>) -> Result<()> {
        let mut state = self.state();
        state.calls.push(Call::Copy(items.into_iter().map(|item| item.id).collect()));
        Self::failure(&state.copy_error)
    }

    fn paste(&self, _: &Storage) -> Result<Payload> {
        let mut state = self.state();
        state.calls.push(Call::Paste);
        state.pasted.pop_front().unwrap_or_else(|| Err(anyhow!("empty clipboard")))
    }

    fn drag(&self, items: Vec<Item>) -> Result<()> {
        let mut state = self.state();
        state.calls.push(Call::Drag(items.into_iter().map(|item| item.id).collect()));
        Self::failure(&state.drag_error)
    }

    fn flush_clipboard(&self) {
        self.record(Call::FlushClipboard);
    }

    fn embed(&self, _: Handler, path: &Path, _: Frame, _: u32, _: u32) -> Result<Box<dyn Embedded>> {
        let mut state = self.state();
        state.calls.push(Call::Embed(path.into()));
        Self::failure(&state.embed_error)?;
        Ok(Box::new(Pane))
    }

    fn set_caption(&self, _: Caption) {}
}

impl Services for Fake {
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) {
        self.state().jobs.push_back(job);
    }

    fn shell_open(&self, target: &str) {
        self.record(Call::ShellOpen(target.into()));
    }

    fn pick(&self, title: &str, folders: bool) -> Result<Option<PathBuf>> {
        let mut state = self.state();
        state.calls.push(Call::Pick(title.into(), folders));
        state.picked.pop_front().unwrap_or(Ok(None))
    }

    fn pick_files(&self, title: &str) -> Result<Vec<PathBuf>> {
        let mut state = self.state();
        state.calls.push(Call::PickFiles(title.into()));
        state.picked_files.pop_front().unwrap_or_else(|| Ok(Vec::new()))
    }

    fn display_name(&self, path: &Path) -> String {
        format!("Shown {}", path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default())
    }

    fn desktop_folder(&self) -> Option<PathBuf> {
        self.state().desktop.clone()
    }

    fn transfer(&self, files: &[PathBuf], folder: &Path, copy: bool) -> Result<()> {
        let mut state = self.state();
        state.calls.push(Call::Transfer(files.to_vec(), folder.into(), copy));
        Self::failure(&state.transfer_error)
    }

    fn thumbnail(&self, _: &Path, _: i32) -> Option<(u32, u32, Vec<u8>)> {
        self.state().thumbnail.clone()
    }

    fn preview_handler(&self, _: &Path) -> Option<Handler> {
        self.state().handler
    }

    fn type_name(&self, _: &Path) -> Option<String> {
        self.state().type_name.clone()
    }

    fn launch(&self, command: &mut process::Command) -> io::Result<()> {
        let mut state = self.state();
        state.calls.push(Call::Launch(Invocation::of(command, None)));
        match state.launch_error {
            Some(kind) => Err(kind.into()),
            None => Ok(()),
        }
    }

    fn output(
        &self,
        command: &mut process::Command,
        input: Option<Vec<u8>>,
        progress: &Progress,
    ) -> io::Result<Output> {
        if progress.cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let mut state = self.state();
        state.calls.push(Call::Output(Invocation::of(command, input)));
        if state.stop_on_output {
            progress.cancel();
        }
        state.outputs.pop_front().unwrap_or_else(|| Err(io::ErrorKind::NotFound.into()))
    }

    fn stream(
        &self,
        command: &mut process::Command,
        input: Option<Vec<u8>>,
        progress: &Progress,
        line: &mut dyn FnMut(&str),
    ) -> io::Result<Output> {
        if progress.cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let invocation = Invocation::of(command, input);
        let streamer = {
            let mut state = self.state();
            state.calls.push(Call::Output(invocation.clone()));
            state.streamer.take()
        };
        let Some(mut streamer) = streamer else {
            return self.state().outputs.pop_front().unwrap_or_else(|| Err(io::ErrorKind::NotFound.into()));
        };
        let result = streamer(&invocation);
        self.state().streamer = Some(streamer);
        if let Ok(output) = &result {
            String::from_utf8_lossy(&output.stdout).lines().for_each(&mut *line);
        }
        result
    }

    fn download(&self, url: &str, target: &Path, progress: &Progress) -> Result<()> {
        let downloader = {
            let mut state = self.state();
            state.calls.push(Call::Download(url.into(), target.into()));
            state.downloader.take()
        };
        let Some(mut downloader) = downloader else {
            return Err(anyhow!("offline"));
        };
        let result = downloader(url, target, progress);
        self.state().downloader = Some(downloader);
        result
    }

    fn serve(&self, files: Vec<PathBuf>) -> Result<Server> {
        let mut state = self.state();
        state.calls.push(Call::Serve(files.clone()));
        Self::failure(&state.serve_error)?;
        Ok(Server::detached(format!("http://192.0.2.10:8000/{}", files.len()), files))
    }

    fn search_path(&self) -> Option<OsString> {
        self.state().search_path.clone()
    }

    fn temp_dir(&self) -> PathBuf {
        self.state().temp.clone()
    }

    fn app_data(&self, place: AppData) -> Option<PathBuf> {
        let state = self.state();
        match place {
            AppData::Roaming => state.roaming.clone(),
            AppData::Local => state.local.clone(),
        }
    }

    fn recognize(&self, image: &RgbaImage, language: &str) -> Result<String> {
        let mut state = self.state();
        state.scanned.push(image.clone());
        state.read_in.push(language.into());
        state.recognized.pop_front().unwrap_or_else(|| Err(anyhow!("no text recognition")))
    }

    fn ocr_languages(&self) -> Vec<(String, String)> {
        self.state().languages.clone()
    }

    fn render_pdf(&self, path: &Path, each: &mut (dyn FnMut(RgbaImage) -> Result<()> + Send)) -> Result<()> {
        let pages = {
            let mut state = self.state();
            state.calls.push(Call::RenderPdf(path.into()));
            state.pages.clone().map_err(|message| anyhow!(message))?
        };
        pages.into_iter().try_for_each(each)
    }

    fn font(&self) -> Option<PathBuf> {
        self.state().font.clone()
    }
}
