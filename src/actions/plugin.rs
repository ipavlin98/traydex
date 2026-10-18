use super::upload::mime;
use super::{Outcome, Progress, base, extension, last_line};
use crate::i18n::{self, fill, tr};
use crate::store::Storage;
use crate::system::{Offer, Services};
use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::Arc;
use std::time::Duration;
use zip::ZipArchive;

mod icon;

pub const BUNDLE: &str = "tdx";
const MANIFEST: &str = "action.json";
const OUTPUT: &str = "output";
const PROTOCOL: u32 = 1;
const DEFAULT_TIMEOUT: u64 = 600;
const ID_LIMIT: usize = 64;
const ENTRY_LIMIT: usize = 4096;
const UNPACKED_LIMIT: u64 = 256 * 1024 * 1024;
const POWERSHELL_PRELUDE: &str = "[Console]::InputEncoding = [Console]::OutputEncoding = \
    New-Object System.Text.UTF8Encoding $false; & $env:TRAYDEX_ENTRY; exit $LASTEXITCODE";

#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    Text,
    Secret,
    Toggle,
    File,
    Folder,
}

#[derive(Debug, Deserialize)]
pub struct Setting {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub input: Input,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    default: Value,
}

impl Setting {
    fn fallback(&self) -> Value {
        match (&self.default, self.input) {
            (Value::Null, Input::Toggle) => Value::Bool(false),
            (Value::Null, Input::Text | Input::Secret | Input::File | Input::Folder) => Value::String(String::new()),
            (value, _) => value.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Accepts {
    extensions: Vec<String>,
    mime: Vec<String>,
    files: bool,
    folders: bool,
}

impl Default for Accepts {
    fn default() -> Self {
        Self { extensions: Vec::new(), mime: Vec::new(), files: true, folders: false }
    }
}

fn mime_matches(pattern: &str, kind: &str) -> bool {
    let kind = kind.split(';').next().unwrap_or_default().trim();
    let pattern = pattern.trim().to_ascii_lowercase();
    match pattern.strip_suffix("/*") {
        Some(group) => kind.split('/').next() == Some(group),
        None => pattern == kind,
    }
}

impl Accepts {
    pub fn admits(&self, path: &Path, folder: bool) -> bool {
        if folder {
            return self.folders;
        }
        if !self.files {
            return false;
        }
        if self.extensions.is_empty() && self.mime.is_empty() {
            return true;
        }
        let own = extension(path);
        self.extensions.iter().any(|wanted| wanted.trim().trim_start_matches('.').eq_ignore_ascii_case(&own))
            || self.mime.iter().any(|pattern| mime_matches(pattern, mime(path)))
    }

    pub fn admits_offer(&self, offer: &Offer) -> bool {
        super::offered(offer, |path, folder| self.admits(path, folder))
    }
}

#[derive(Debug, Deserialize)]
struct Manifest {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon: Option<PathBuf>,
    entry: PathBuf,
    #[serde(default)]
    accepts: Accepts,
    #[serde(default)]
    settings: Vec<Setting>,
    #[serde(default)]
    timeout: Option<u64>,
}

fn inside(path: &Path) -> bool {
    path.components().next().is_some() && path.components().all(|part| matches!(part, Component::Normal(_)))
}

fn valid_id(id: &str) -> bool {
    (1..=ID_LIMIT).contains(&id.len())
        && !id.starts_with('.')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

impl Manifest {
    fn parse(data: impl Read) -> Result<Self> {
        let manifest: Self = serde_json::from_reader(data).context(MANIFEST)?;
        manifest.validate().context(MANIFEST)?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<()> {
        ensure!(valid_id(&self.id), "\"id\" must be 1–{ID_LIMIT} lowercase latin letters, digits, '-', '_' or '.'");
        ensure!(!self.name.trim().is_empty(), "\"name\" is empty");
        ensure!(inside(&self.entry), "\"entry\" must be a path inside the plugin folder");
        ensure!(Runner::of(&self.entry).is_some(), "\"entry\" must be a .ps1, .py, .bat, .cmd or .exe file");
        ensure!(self.icon.as_deref().is_none_or(inside), "\"icon\" must be a path inside the plugin folder");
        ensure!(self.icon.as_deref().is_none_or(|icon| extension(icon) == "svg"), "\"icon\" must be an .svg file");
        ensure!(self.timeout != Some(0), "\"timeout\" must be at least one second");
        for (index, setting) in self.settings.iter().enumerate() {
            let key = &setting.key;
            ensure!(
                !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "setting \"{key}\": the key must be latin letters, digits or '_'"
            );
            ensure!(
                !self.settings.iter().take(index).any(|other| other.key.eq_ignore_ascii_case(key)),
                "setting \"{key}\" is declared twice"
            );
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Plugin {
    dir: PathBuf,
    icon: Option<Arc<str>>,
    manifest: Manifest,
}

impl Plugin {
    fn load(dir: &Path) -> Result<Self> {
        let manifest = Manifest::parse(fs::File::open(dir.join(MANIFEST)).context(MANIFEST)?)?;
        let entry = dir.join(&manifest.entry);
        ensure!(entry.is_file(), "{}: the entry file is missing", manifest.entry.display());
        let icon = manifest.icon.as_ref().and_then(|icon| {
            icon::load(&dir.join(icon))
                .inspect_err(|error| {
                    log::warn!("Skipped the icon {} of \"{}\": {error:#}", icon.display(), manifest.id)
                })
                .ok()
        });
        Ok(Self { dir: dir.to_path_buf(), icon, manifest })
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn name(&self) -> &str {
        self.manifest.name.trim()
    }

    pub fn description(&self) -> &str {
        self.manifest.description.trim()
    }

    pub fn settings(&self) -> &[Setting] {
        &self.manifest.settings
    }

    pub fn accepts(&self) -> &Accepts {
        &self.manifest.accepts
    }

    pub fn icon(&self) -> Option<Arc<str>> {
        self.icon.clone()
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.manifest.timeout.unwrap_or(DEFAULT_TIMEOUT))
    }

    fn entry(&self) -> PathBuf {
        self.dir.join(&self.manifest.entry)
    }
}

fn folder(storage: &Storage) -> PathBuf {
    storage.root().join("actions").join("custom")
}

pub fn scan(storage: &Storage) -> Vec<Arc<Plugin>> {
    let Ok(entries) = fs::read_dir(folder(storage)) else {
        return Vec::new();
    };
    let mut plugins: Vec<Arc<Plugin>> = entries
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.') && entry.path().is_dir())
        .filter_map(|entry| {
            let dir = entry.path();
            let loaded = Plugin::load(&dir).and_then(|plugin| {
                ensure!(entry.file_name() == plugin.id(), "the folder name differs from the id \"{}\"", plugin.id());
                Ok(plugin)
            });
            loaded.inspect_err(|error| log::warn!("Skipped the plugin in {}: {error:#}", dir.display())).ok()
        })
        .map(Arc::new)
        .collect();
    plugins.sort_by_key(|plugin| plugin.name().to_lowercase());
    plugins
}

fn remove_tree(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn extract(zip: &mut ZipArchive<fs::File>, target: &Path, limit: u64) -> Result<()> {
    let mut left = limit;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.enclosed_name().with_context(|| {
            format!("{}: the path leaves the plugin folder", String::from_utf8_lossy(entry.name_raw()))
        })?;
        let path = target.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let copied = io::copy(&mut (&mut entry).take(left + 1), &mut fs::File::create(&path)?)?;
        ensure!(copied <= left, "the plugin unpacks to more than {} MB", limit >> 20);
        left -= copied;
    }
    Ok(())
}

fn replace(staged: &Path, target: &Path, old: &Path) -> Result<()> {
    remove_tree(old)?;
    let existed = target.exists();
    if existed {
        fs::rename(target, old)?;
    }
    if let Err(error) = fs::rename(staged, target) {
        if existed && let Err(restore) = fs::rename(old, target) {
            log::error!("Couldn't restore the previous plugin version: {restore}");
        }
        return Err(error.into());
    }
    if existed && let Err(error) = remove_tree(old) {
        log::warn!("Couldn't remove the previous plugin version: {error}");
    }
    Ok(())
}

fn unpack(storage: &Storage, bundle: &Path) -> Result<Arc<Plugin>> {
    let mut zip = ZipArchive::new(fs::File::open(bundle)?)?;
    ensure!(zip.len() <= ENTRY_LIMIT, "the plugin has more than {ENTRY_LIMIT} files");
    let id = Manifest::parse(zip.by_name(MANIFEST).context(MANIFEST)?)?.id;
    let root = folder(storage);
    fs::create_dir_all(&root)?;
    let staged = root.join(format!(".{id}.new"));
    remove_tree(&staged)?;
    let target = root.join(&id);
    let installed =
        extract(&mut zip, &staged, UNPACKED_LIMIT).and_then(|()| Plugin::load(&staged)).and_then(|plugin| {
            ensure!(plugin.id() == id, "action.json changed while unpacking");
            replace(&staged, &target, &root.join(format!(".{id}.old")))?;
            Ok(Plugin { dir: target, ..plugin })
        });
    if installed.is_err()
        && let Err(error) = remove_tree(&staged)
    {
        log::warn!("Couldn't clean up the unpacked plugin: {error}");
    }
    installed.map(Arc::new)
}

pub fn install(storage: &Storage, bundle: &Path) -> Result<Arc<Plugin>> {
    unpack(storage, bundle).with_context(|| fill(tr().plugin_install_failed, base(bundle)))
}

fn discard(plugin: &Plugin) -> io::Result<()> {
    let old = plugin.dir.with_file_name(format!(".{}.old", plugin.id()));
    remove_tree(&old)?;
    match fs::rename(&plugin.dir, &old) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        result => result?,
    }
    if let Err(error) = remove_tree(&old) {
        log::warn!("Couldn't remove the files of the uninstalled plugin: {error}");
    }
    Ok(())
}

pub fn uninstall(plugin: &Plugin) -> Result<()> {
    discard(plugin).with_context(|| fill(tr().plugin_uninstall_failed, plugin.name()))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Custom {
    pub id: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    values: BTreeMap<String, Value>,
    #[serde(skip)]
    plugin: Option<Arc<Plugin>>,
}

fn filled(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.trim().is_empty(),
        Value::Null => false,
        _ => true,
    }
}

impl Custom {
    pub fn new(plugin: Arc<Plugin>) -> Self {
        Self { id: plugin.id().to_string(), values: BTreeMap::new(), plugin: Some(plugin) }
    }

    pub fn plugin(&self) -> Option<&Plugin> {
        self.plugin.as_deref()
    }

    pub(super) fn attach(&mut self, plugins: &[Arc<Plugin>]) {
        self.plugin = plugins.iter().find(|plugin| plugin.id() == self.id).cloned();
    }

    pub fn value(&self, setting: &Setting) -> Value {
        self.values.get(&setting.key).filter(|value| !value.is_null()).cloned().unwrap_or_else(|| setting.fallback())
    }

    pub fn text(&self, setting: &Setting) -> String {
        match self.value(setting) {
            Value::String(text) => text,
            Value::Null => String::new(),
            other => other.to_string(),
        }
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
    }

    pub fn ready(&self) -> bool {
        self.plugin().is_some_and(|plugin| {
            plugin.settings().iter().filter(|setting| setting.required).all(|setting| filled(&self.value(setting)))
        })
    }

    fn values(&self, plugin: &Plugin) -> serde_json::Map<String, Value> {
        plugin.settings().iter().map(|setting| (setting.key.clone(), self.value(setting))).collect()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Runner {
    PowerShell,
    Python,
    Direct,
}

impl Runner {
    fn of(entry: &Path) -> Option<Self> {
        Some(match extension(entry).as_str() {
            "ps1" => Runner::PowerShell,
            "py" => Runner::Python,
            "bat" | "cmd" | "exe" => Runner::Direct,
            _ => return None,
        })
    }

    fn command(self, entry: &Path) -> Command {
        match self {
            Runner::PowerShell => {
                let mut shell = Command::new("powershell.exe");
                shell.args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    POWERSHELL_PRELUDE,
                ]);
                shell.env("TRAYDEX_ENTRY", entry);
                shell
            }
            Runner::Python => {
                let mut python = Command::new("python.exe");
                python.arg(entry);
                python
            }
            Runner::Direct => Command::new(entry),
        }
    }
}

fn env_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(on) => if *on { "1" } else { "0" }.into(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn env_key(key: &str) -> String {
    format!("TRAYDEX_SETTING_{}", key.to_ascii_uppercase())
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Message {
    Progress { value: f32 },
    Clipboard { text: String },
    Add { path: PathBuf },
    Notify { text: String },
    Error { text: String },
}

#[derive(Default)]
struct Feedback {
    created: Vec<PathBuf>,
    clipboard: Option<String>,
    notice: Option<String>,
    error: Option<String>,
}

impl Feedback {
    fn hear(&mut self, line: &str, output: &Path, progress: &Progress) {
        let line = line.trim();
        if !line.starts_with('{') {
            return;
        }
        let message = match serde_json::from_str::<Message>(line) {
            Ok(message) => message,
            Err(error) => return log::debug!("Not a plugin message ({error}): {line}"),
        };
        match message {
            Message::Progress { value } if value.is_finite() => {
                progress.update(|stage| stage.fraction = Some(value.clamp(0., 1.)));
            }
            Message::Progress { .. } => {}
            Message::Clipboard { text } => self.clipboard = Some(text),
            Message::Add { path } => self.created.push(output.join(path)),
            Message::Notify { text } => self.notice = Some(text),
            Message::Error { text } => self.error = Some(text),
        }
    }

    fn finish(mut self, status: ExitStatus, stderr: &[u8], title: &str) -> Result<Outcome> {
        if let Some(error) = self.error.filter(|error| !error.trim().is_empty()) {
            bail!(error);
        }
        if !status.success() {
            let line = last_line(stderr);
            bail!(if line.is_empty() { fill(tr().plugin_exit, status.code().unwrap_or(-1)) } else { line });
        }
        let mut created = Vec::new();
        for path in self.created.drain(..).filter(|path| path.exists()) {
            if !created.contains(&path) {
                created.push(path);
            }
        }
        let message = match self.notice.filter(|notice| !notice.trim().is_empty()) {
            Some(notice) => notice,
            None if !created.is_empty() => fill(tr().converted, created.len()),
            None => fill(tr().plugin_done, title),
        };
        Ok(Outcome { message, created, clipboard: self.clipboard, show: None })
    }
}

fn invoke(
    plugin: &Plugin,
    custom: &Custom,
    title: &str,
    files: &[&PathBuf],
    output: &Path,
    progress: &Progress,
    system: &dyn Services,
) -> Result<Outcome> {
    let entry = plugin.entry();
    let runner = Runner::of(&entry).with_context(|| fill(tr().program_missing, entry.display()))?;
    let values = custom.values(plugin);
    let language = i18n::current().code();
    let input = json!({
        "protocol": PROTOCOL,
        "action": plugin.id(),
        "files": files,
        "settings": values,
        "output": output,
        "language": language,
    });
    let mut command = runner.command(&entry);
    command
        .current_dir(&plugin.dir)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("TRAYDEX_PROTOCOL", PROTOCOL.to_string())
        .env("TRAYDEX_ACTION", plugin.id())
        .env("TRAYDEX_ACTION_DIR", &plugin.dir)
        .env("TRAYDEX_OUTPUT", output)
        .env("TRAYDEX_LANGUAGE", language);
    for (key, value) in &values {
        command.env(env_key(key), env_value(value));
    }
    let mut feedback = Feedback::default();
    let ran = system.stream(&mut command, Some(serde_json::to_vec(&input)?), progress, &mut |line| {
        feedback.hear(line, output, progress)
    });
    let finished = match ran {
        Err(error) if error.kind() == io::ErrorKind::NotFound && runner == Runner::Python => {
            return Err(anyhow!(fill(tr().plugin_runtime, "Python")));
        }
        result => result?,
    };
    feedback.finish(finished.status, &finished.stderr, title)
}

pub(super) fn run(
    custom: &Custom,
    title: &str,
    files: &[PathBuf],
    progress: &Progress,
    storage: &Storage,
    system: &dyn Services,
) -> Result<Outcome> {
    let plugin = custom.plugin().with_context(|| fill(tr().plugin_missing, &custom.id))?;
    let files: Vec<&PathBuf> = files.iter().filter(|file| plugin.accepts().admits(file, file.is_dir())).collect();
    ensure!(!files.is_empty(), tr().plugin_unsuitable);
    let output = storage.allocate(OUTPUT)?;
    fs::create_dir_all(&output)?;
    let result = invoke(plugin, custom, title, &files, &output, progress, system);
    let kept = result.as_ref().is_ok_and(|outcome| outcome.created.iter().any(|path| path.starts_with(&output)));
    if !kept {
        storage.release(&output);
    }
    result
}

#[cfg(test)]
mod tests;
