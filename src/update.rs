use crate::actions::{CONNECT_TIMEOUT, curl_exe, curl_failure};
use crate::i18n::{self, tr};
use crate::profile::{self, PORTABLE_MARKER};
use crate::system::{Progress, Services};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::cmp::Ordering;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipArchive;

pub const WAIT_FLAG: &str = "--after-update";
const RELEASES: &str = "https://api.github.com/repos/ipavlin98/traydex/releases?per_page=30";
const ACCEPT: &str = "Accept: application/vnd.github+json";
const CHECK_TIMEOUT: &str = "60";
const SUMS: &str = "SHA256SUMS.txt";
const EXE: &str = "Traydex.exe";
const FOLDER: &str = "Traydex-update";
const OLD: &str = ".update-old";
const NEW: &str = ".update-new";
const SETUP_FLAGS: [&str; 4] = ["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/relaunch=1"];
const SETUP_LANGUAGES: [&str; 3] = ["ru", "es", "fr"];
const SETUP_FALLBACK: &str = "en";

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Part {
    Number(u64),
    Text(String),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Version {
    core: [u64; 3],
    pre: Vec<Part>,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let text = text.split_once('+').map_or(text, |(version, _)| version);
        let (core, pre) = text.split_once('-').map_or((text, None), |(core, pre)| (core, Some(pre)));
        let mut numbers = core.split('.').map(|number| number.parse::<u64>().ok());
        let core = [numbers.next()??, numbers.next()??, numbers.next()??];
        if numbers.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre.split('.').map(part).collect::<Option<_>>()?,
        };
        Some(Self { core, pre })
    }

    pub fn current() -> Option<Self> {
        Self::parse(env!("CARGO_PKG_VERSION"))
    }

    fn preview(&self) -> bool {
        !self.pre.is_empty()
    }
}

fn part(text: &str) -> Option<Part> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some(text.parse().map_or_else(|_| Part::Text(text.into()), Part::Number))
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| match (self.preview(), other.preview()) {
            (false, false) => Ordering::Equal,
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (true, true) => self.pre.cmp(&other.pre),
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let [major, minor, patch] = self.core;
        write!(f, "{major}.{minor}.{patch}")?;
        for (index, part) in self.pre.iter().enumerate() {
            f.write_str(if index == 0 { "-" } else { "." })?;
            match part {
                Part::Number(number) => write!(f, "{number}")?,
                Part::Text(text) => f.write_str(text)?,
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Arch {
    X64,
    X86,
    Arm64,
}

impl Arch {
    const CURRENT: Arch = if cfg!(target_arch = "aarch64") {
        Arch::Arm64
    } else if cfg!(target_arch = "x86") {
        Arch::X86
    } else {
        Arch::X64
    };

    fn name(self) -> &'static str {
        match self {
            Arch::X64 => "x64",
            Arch::X86 => "x86",
            Arch::Arm64 => "arm64",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    Setup,
    Portable,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Channel {
    Stable,
    Preview,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Install {
    arch: Arch,
    form: Form,
    channel: Channel,
}

impl Install {
    fn current(version: &Version) -> Self {
        Self {
            arch: Arch::CURRENT,
            form: if profile::current().portable() { Form::Portable } else { Form::Setup },
            channel: if version.preview() { Channel::Preview } else { Channel::Stable },
        }
    }

    fn asset(self, version: &Version) -> String {
        let arch = self.arch.name();
        match self.form {
            Form::Setup => format!("Traydex-{version}-{arch}-setup.exe"),
            Form::Portable => format!("Traydex-{version}-{arch}-portable.zip"),
        }
    }

    fn accepts(self, version: &Version) -> bool {
        self.channel == Channel::Preview || !version.preview()
    }
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct Listed {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Release {
    pub version: Version,
    form: Form,
    file: Asset,
    sums: String,
}

fn newest(json: &str, install: Install, current: &Version) -> Result<Option<Release>> {
    let listed: Vec<Listed> = serde_json::from_str(json).context("GitHub returned an unexpected release list")?;
    let usable = listed.into_iter().filter(|listed| !listed.draft).filter_map(|listed| {
        let version = Version::parse(&listed.tag_name)?;
        if version <= *current || !install.accepts(&version) {
            return None;
        }
        let url = |name: &str| {
            listed.assets.iter().find(|asset| asset.name == name).map(|asset| asset.browser_download_url.clone())
        };
        let name = install.asset(&version);
        let file = Asset { browser_download_url: url(&name)?, name };
        Some(Release { sums: url(SUMS)?, version, form: install.form, file })
    });
    Ok(usable.max_by(|a, b| a.version.cmp(&b.version)))
}

pub fn check(system: &dyn Services) -> Result<Option<Release>> {
    let current = Version::current().context("the app version isn't a valid version")?;
    let install = Install::current(&current);
    log::info!("Checking for updates to {current}, {install:?}");
    let mut command = Command::new(curl_exe());
    command.args(["-sSfL", "--connect-timeout", CONNECT_TIMEOUT, "--max-time", CHECK_TIMEOUT, "-H", ACCEPT, RELEASES]);
    let output = system.output(&mut command, None, &Progress::default())?;
    if !output.status.success() {
        bail!(curl_failure(output.status.code(), &output.stderr));
    }
    newest(&String::from_utf8_lossy(&output.stdout), install, &current)
}

pub fn prepare(release: &Release, system: &dyn Services, progress: &Progress) -> Result<Command> {
    let folder = system.temp_dir().join(FOLDER);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder)?;
    let sums = folder.join(SUMS);
    system.download(&release.sums, &sums, progress)?;
    let expected = expected_hash(&fs::read_to_string(&sums)?, &release.file.name).context(tr().update_corrupted)?;
    let file = folder.join(&release.file.name);
    system.download(&release.file.browser_download_url, &file, progress)?;
    if sha256(&file)? != expected {
        bail!(tr().update_corrupted);
    }
    match release.form {
        Form::Setup => Ok(setup_command(&file)),
        Form::Portable => {
            let exe = std::env::current_exe()?;
            replace(&file, &exe)?;
            let mut command = Command::new(exe);
            command.arg(WAIT_FLAG);
            Ok(command)
        }
    }
}

fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        (file.trim_start().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

fn sha256(file: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(file)?, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn setup_command(file: &Path) -> Command {
    let code = i18n::current().code();
    let language = SETUP_LANGUAGES.into_iter().find(|known| *known == code).unwrap_or(SETUP_FALLBACK);
    let mut command = Command::new(file);
    command.args(SETUP_FLAGS).arg(format!("/LANG={language}"));
    command
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path);
    name.push(suffix);
    name.into()
}

fn replace(archive: &Path, exe: &Path) -> Result<()> {
    let folder = exe.parent().context("the app has no folder")?;
    let mut fresh = Vec::new();
    let unpacked = unpack(archive, exe, folder, &mut fresh);
    if unpacked.is_err() || !fresh.iter().any(|(target, _)| target == exe) {
        discard(&fresh);
        unpacked?;
        bail!(tr().update_corrupted);
    }
    fresh.sort_by_key(|(target, _)| target != exe);
    let (main, rest) = fresh.split_first().context(tr().update_corrupted)?;
    if let Err(error) = swap(&main.0, &main.1) {
        discard(&fresh);
        return Err(error.into());
    }
    for (target, path) in rest {
        if let Err(error) = swap(target, path) {
            log::warn!("Couldn't update {}: {error}", target.display());
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

fn discard(fresh: &[(PathBuf, PathBuf)]) {
    for (_, path) in fresh {
        let _ = fs::remove_file(path);
    }
}

fn unpack(archive: &Path, exe: &Path, folder: &Path, fresh: &mut Vec<(PathBuf, PathBuf)>) -> Result<()> {
    let mut zip = ZipArchive::new(File::open(archive)?)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let Some(name) = entry.enclosed_name().and_then(|path| path.file_name().map(ToOwned::to_owned)) else {
            continue;
        };
        if !entry.is_file() || name == PORTABLE_MARKER {
            continue;
        }
        let target = if name.eq_ignore_ascii_case(EXE) { exe.to_path_buf() } else { folder.join(&name) };
        let path = suffixed(&target, NEW);
        let mut output = File::create(&path)?;
        fresh.push((target, path));
        io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

fn swap(target: &Path, fresh: &Path) -> io::Result<()> {
    let old = suffixed(target, OLD);
    let existed = target.exists();
    if existed {
        fs::rename(target, &old)?;
    }
    fs::rename(fresh, target).inspect_err(|_| {
        if existed {
            let _ = fs::rename(&old, target);
        }
    })
}

pub fn sweep(temp: &Path, exe: &Path) {
    let _ = fs::remove_dir_all(temp.join(FOLDER));
    let Some(Ok(entries)) = exe.parent().map(fs::read_dir) else {
        return;
    };
    for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if (name.ends_with(OLD) || name.ends_with(NEW)) && fs::remove_file(&path).is_ok() {
            log::info!("Removed a file left by an update: {}", path.display());
        }
    }
}

#[cfg(test)]
mod tests;
