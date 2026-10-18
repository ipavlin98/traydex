use super::{Outcome, Progress, Show, base, output, stem};
use crate::i18n::{fill, tr};
use crate::os;
use crate::store::{self, Storage};
use anyhow::{Context, Result, bail};
use sevenz_rust2::{ArchiveReader, EncoderMethod, Password};
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use unrar::VolumeInfo;
use unrar::error::{Code, UnrarError};
use zip::ZipArchive;
use zip::result::ZipError;

const SNIFF: u64 = 512;
const TAR_MAGIC: std::ops::Range<usize> = 257..262;
const CHUNK: usize = 64 * 1024;
const FORBIDDEN: &str = r#"<>:"|?*"#;
const STAGING: &str = "unpacking";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Format {
    Zip,
    SevenZip,
    Rar,
    Tar,
    Packed(Codec),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Codec {
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

enum Access {
    Open,
    Locked,
}

#[derive(Default)]
pub struct Unlocked {
    pub created: Vec<PathBuf>,
    pub locked: Vec<PathBuf>,
    pub failed: usize,
}

pub(super) fn unpack(files: &[PathBuf], progress: &Progress, storage: &Storage) -> Result<Outcome> {
    let archives: Vec<PathBuf> = files.iter().filter(|file| detect(file).is_some()).cloned().collect();
    if archives.is_empty() {
        bail!(tr().unpack_needs);
    }
    let unlocked = unlock(&archives, "", progress, storage)?;
    let mut message = match unlocked.created.len() {
        0 => String::new(),
        count => fill(tr().unpacked, count),
    };
    let skipped = files.len() - archives.len() + unlocked.failed;
    if skipped > 0 && !message.is_empty() {
        message = format!("{message} · {}", fill(tr().skipped, skipped));
    }
    let show = (!unlocked.locked.is_empty()).then_some(Show::Password(unlocked.locked));
    Ok(Outcome { message, created: unlocked.created, clipboard: None, show })
}

pub fn unlock(archives: &[PathBuf], password: &str, progress: &Progress, storage: &Storage) -> Result<Unlocked> {
    let mut unlocked = Unlocked::default();
    let mut error = None;
    for (index, archive) in archives.iter().enumerate() {
        if progress.cancelled() {
            break;
        }
        let mut gauge = Gauge::new(progress, index, archives.len());
        match open(archive, password, &mut gauge, storage) {
            Ok(Some(path)) => unlocked.created.push(path),
            Ok(None) => unlocked.locked.push(archive.clone()),
            Err(failure) => {
                log::warn!("{}: {failure:#}", archive.display());
                unlocked.failed += 1;
                error.get_or_insert(failure.context(base(archive)));
            }
        }
    }
    if progress.cancelled() {
        unlocked.created.iter().for_each(|path| scrap(path, storage));
        bail!(tr().stopped);
    }
    match error {
        Some(error) if unlocked.created.is_empty() && unlocked.locked.is_empty() => Err(error),
        _ => Ok(unlocked),
    }
}

fn open(archive: &Path, password: &str, gauge: &mut Gauge, storage: &Storage) -> Result<Option<PathBuf>> {
    let format = detect(archive).context(tr().unpack_needs)?;
    let progress = gauge.progress;
    let mut place = Place { archive, storage, folder: None };
    let access = match format {
        Format::Zip => zip(archive, password, gauge, &mut place),
        Format::SevenZip => seven(archive, password, gauge, &mut place),
        Format::Rar => rar(archive, password, gauge, &mut place),
        Format::Tar => counted(archive, gauge).and_then(|source| tar(source, &mut place, progress)),
        Format::Packed(codec) if packs_tar(archive, codec) => {
            counted(archive, gauge).and_then(|source| tar(decoder(codec, source)?, &mut place, progress))
        }
        Format::Packed(codec) => return single(archive, codec, gauge, storage).map(Some),
    };
    match access {
        Ok(Access::Open) => settle(place.folder()?).map(Some),
        Ok(Access::Locked) => {
            place.discard();
            Ok(None)
        }
        Err(error) => {
            place.discard();
            Err(error)
        }
    }
}

fn detect(path: &Path) -> Option<Format> {
    let mut head = Vec::new();
    fs::File::open(path).ok()?.take(SNIFF).read_to_end(&mut head).ok()?;
    format(&head)
}

fn format(head: &[u8]) -> Option<Format> {
    let starts = |magic: &[u8]| head.starts_with(magic);
    Some(if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") {
        Format::Zip
    } else if starts(b"7z\xBC\xAF\x27\x1C") {
        Format::SevenZip
    } else if starts(b"Rar!\x1A\x07") {
        Format::Rar
    } else if starts(&[0x1F, 0x8B]) {
        Format::Packed(Codec::Gzip)
    } else if starts(b"BZh") {
        Format::Packed(Codec::Bzip2)
    } else if starts(b"\xFD7zXZ\x00") {
        Format::Packed(Codec::Xz)
    } else if starts(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Format::Packed(Codec::Zstd)
    } else if tarred(head) {
        Format::Tar
    } else {
        return None;
    })
}

fn tarred(head: &[u8]) -> bool {
    head.get(TAR_MAGIC) == Some(b"ustar")
}

fn packs_tar(archive: &Path, codec: Codec) -> bool {
    let mut head = Vec::new();
    let read = fs::File::open(archive)
        .map_err(anyhow::Error::from)
        .and_then(|file| decoder(codec, file))
        .and_then(|source| Ok(source.take(SNIFF).read_to_end(&mut head)?));
    read.is_ok() && tarred(&head)
}

fn decoder<'a>(codec: Codec, source: impl Read + 'a) -> Result<Box<dyn Read + 'a>> {
    Ok(match codec {
        Codec::Gzip => Box::new(flate2::read::MultiGzDecoder::new(source)),
        Codec::Bzip2 => Box::new(bzip2::read::MultiBzDecoder::new(source)),
        Codec::Xz => Box::new(lzma_rust2::XzReader::new(source, true)),
        Codec::Zstd => Box::new(zstd::stream::read::Decoder::new(source)?),
    })
}

struct Gauge<'a> {
    progress: &'a Progress,
    start: f32,
    span: f32,
    total: u64,
    done: u64,
}

impl<'a> Gauge<'a> {
    fn new(progress: &'a Progress, index: usize, count: usize) -> Self {
        let count = count.max(1) as f32;
        Self { progress, start: index as f32 / count, span: 1. / count, total: 0, done: 0 }
    }

    fn advance(&mut self, bytes: u64) {
        self.done += bytes;
        if self.total > 0 {
            let part = self.done.min(self.total) as f32 / self.total as f32;
            let fraction = self.start + self.span * part;
            self.progress.update(|stage| stage.fraction = Some(fraction));
        }
    }
}

struct Counted<'a, 'b, R> {
    inner: R,
    gauge: &'b mut Gauge<'a>,
}

impl<R: Read> Read for Counted<'_, '_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.gauge.advance(read as u64);
        Ok(read)
    }
}

fn counted<'a, 'b>(archive: &Path, gauge: &'b mut Gauge<'a>) -> Result<BufReader<Counted<'a, 'b, fs::File>>> {
    let file = fs::File::open(archive)?;
    gauge.total = file.metadata()?.len();
    Ok(BufReader::new(Counted { inner: file, gauge }))
}

struct Place<'a> {
    archive: &'a Path,
    storage: &'a Storage,
    folder: Option<PathBuf>,
}

impl Place<'_> {
    fn folder(&mut self) -> Result<PathBuf> {
        if let Some(folder) = &self.folder {
            return Ok(folder.clone());
        }
        let folder = destination(self.archive, self.storage)?;
        self.folder = Some(folder.clone());
        Ok(folder)
    }

    fn discard(&self) {
        if let Some(folder) = &self.folder {
            scrap(folder, self.storage);
        }
    }
}

fn destination(archive: &Path, storage: &Storage) -> Result<PathBuf> {
    let name = folder_name(archive);
    if let Some(parent) = archive.parent().filter(|_| !storage.owned(archive)) {
        let target = store::vacant(parent.join(&name));
        match fs::create_dir(&target) {
            Ok(()) => return Ok(target),
            Err(error) => log::info!("{} isn't writable ({error}), unpacking to Traydex storage", parent.display()),
        }
    }
    let target = storage.allocate(&name)?;
    fs::create_dir(&target)?;
    Ok(target)
}

fn folder_name(archive: &Path) -> String {
    let name = stem(archive);
    let inner = Path::new(&name);
    let name = if super::extension(inner) == "tar" { stem(inner) } else { name };
    if name.trim().is_empty() { tr().archive.into() } else { name }
}

fn settle(folder: PathBuf) -> Result<PathBuf> {
    let mut entries = fs::read_dir(&folder)?;
    let (Some(only), None) = (entries.next(), entries.next()) else {
        return Ok(folder);
    };
    let name = only?.file_name();
    let parent = folder.parent().context(tr().unsupported)?;
    let staging = store::vacant(folder.with_file_name(format!("{}.{STAGING}", base(&folder))));
    fs::rename(&folder, &staging)?;
    let target = store::vacant(parent.join(&name));
    fs::rename(staging.join(&name), &target)?;
    fs::remove_dir(&staging)?;
    Ok(target)
}

fn scrap(path: &Path, storage: &Storage) {
    if storage.owned(path) {
        return storage.release(path);
    }
    let removed = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
    if let Err(error) = removed {
        log::warn!("Couldn't remove {}: {error}", path.display());
    }
}

fn inside(root: &Path, name: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    let mut named = false;
    for part in name.split(['/', '\\']) {
        if part == ".." {
            return None;
        }
        let clean: String =
            part.chars().map(|c| if c.is_control() || FORBIDDEN.contains(c) { '_' } else { c }).collect();
        let clean = clean.trim_end_matches(['.', ' ']);
        if !clean.is_empty() {
            path.push(clean);
            named = true;
        }
    }
    named.then_some(path)
}

enum Spill {
    Read(io::Error),
    Write(io::Error),
}

impl Spill {
    fn error(self) -> io::Error {
        match self {
            Spill::Read(error) | Spill::Write(error) => error,
        }
    }
}

fn write(source: &mut dyn Read, target: &Path, progress: &Progress) -> Result<(), Spill> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(Spill::Write)?;
    }
    let mut file = fs::File::create(target).map_err(Spill::Write)?;
    let mut buffer = vec![0; CHUNK];
    loop {
        progress.ensure_running().map_err(Spill::Write)?;
        let read = match source.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(Spill::Read(error)),
        };
        let chunk = buffer.get(..read).ok_or_else(|| Spill::Read(io::ErrorKind::InvalidData.into()))?;
        file.write_all(chunk).map_err(Spill::Write)?;
    }
}

fn single(archive: &Path, codec: Codec, gauge: &mut Gauge, storage: &Storage) -> Result<PathBuf> {
    let progress = gauge.progress;
    let target = output(archive, &stem(archive), storage)?;
    let written = counted(archive, gauge)
        .and_then(|source| decoder(codec, source))
        .and_then(|mut source| write(&mut source, &target, progress).map_err(|spill| spill.error().into()));
    if let Err(error) = written {
        scrap(&target, storage);
        return Err(error);
    }
    Ok(target)
}

fn tar(source: impl Read, place: &mut Place, progress: &Progress) -> Result<Access> {
    let folder = place.folder()?;
    let mut archive = tar::Archive::new(source);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        let Some(target) = inside(&folder, &String::from_utf8_lossy(&entry.path_bytes())) else {
            continue;
        };
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
        } else if kind.is_file() || kind.is_contiguous() {
            write(&mut entry, &target, progress).map_err(Spill::error)?;
        }
    }
    Ok(Access::Open)
}

fn entry_name(raw: &[u8]) -> String {
    std::str::from_utf8(raw).map_or_else(|_| os::oem_string(raw), str::to_string)
}

fn zip(archive: &Path, password: &str, gauge: &mut Gauge, place: &mut Place) -> Result<Access> {
    let progress = gauge.progress;
    let mut zip = ZipArchive::new(fs::File::open(archive)?)?;
    let mut encrypted = false;
    for index in 0..zip.len() {
        let entry = zip.by_index_raw(index)?;
        encrypted |= entry.encrypted();
        gauge.total += entry.size();
    }
    if encrypted && password.is_empty() {
        return Ok(Access::Locked);
    }
    let folder = place.folder()?;
    for index in 0..zip.len() {
        let entry =
            if password.is_empty() { zip.by_index(index) } else { zip.by_index_decrypt(index, password.as_bytes()) };
        let mut entry = match entry {
            Err(ZipError::InvalidPassword) => return Ok(Access::Locked),
            entry => entry?,
        };
        let Some(target) = inside(&folder, &entry_name(entry.name_raw())) else {
            continue;
        };
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if entry.is_symlink() {
            continue;
        }
        let sealed = entry.encrypted();
        match write(&mut Counted { inner: &mut entry, gauge: &mut *gauge }, &target, progress) {
            Ok(()) => {}
            Err(Spill::Read(_)) if sealed => return Ok(Access::Locked),
            Err(spill) => return Err(spill.error().into()),
        }
    }
    Ok(Access::Open)
}

fn seven(archive: &Path, password: &str, gauge: &mut Gauge, place: &mut Place) -> Result<Access> {
    use sevenz_rust2::Error;
    let progress = gauge.progress;
    let secret = if password.is_empty() { Password::empty() } else { Password::from(password) };
    let mut reader = match ArchiveReader::open(archive, secret) {
        Ok(reader) => reader,
        Err(Error::PasswordRequired | Error::MaybeBadPassword(_)) => return Ok(Access::Locked),
        Err(_) if !password.is_empty() && sealed_headers(archive) => return Ok(Access::Locked),
        Err(error) => return Err(error.into()),
    };
    let contents = reader.archive();
    let encrypted = contents
        .blocks
        .iter()
        .flat_map(|block| &block.coders)
        .any(|coder| coder.encoder_method_id() == EncoderMethod::ID_AES256_SHA256);
    if encrypted && password.is_empty() {
        return Ok(Access::Locked);
    }
    gauge.total = contents.files.iter().map(|entry| entry.size()).sum();
    let folder = place.folder()?;
    let mut failure = None;
    let read = reader.for_each_entries(|entry, source| {
        let Some(target) = inside(&folder, entry.name()).filter(|_| !entry.is_anti_item()) else {
            return Ok(true);
        };
        let written = if entry.is_directory() {
            fs::create_dir_all(&target).map_err(Spill::Write)
        } else {
            write(&mut Counted { inner: source, gauge: &mut *gauge }, &target, progress)
        };
        match written {
            Ok(()) => Ok(true),
            Err(Spill::Read(error)) => Err(error.into()),
            Err(Spill::Write(error)) => {
                failure = Some(error);
                Ok(false)
            }
        }
    });
    if let Some(error) = failure {
        return Err(error.into());
    }
    match read {
        Ok(()) => Ok(Access::Open),
        Err(Error::MaybeBadPassword(_)) if encrypted => Ok(Access::Locked),
        Err(error) => Err(error.into()),
    }
}

fn sealed_headers(archive: &Path) -> bool {
    matches!(ArchiveReader::open(archive, Password::empty()), Err(sevenz_rust2::Error::PasswordRequired))
}

fn rar(archive: &Path, password: &str, gauge: &mut Gauge, place: &mut Place) -> Result<Access> {
    let progress = gauge.progress;
    let secret = os::ansi_bytes(&password.replace('\0', ""));
    let opened = || {
        if password.is_empty() { unrar::Archive::new(archive) } else { unrar::Archive::with_password(archive, &secret) }
    };
    let Some(encrypted) = survey(opened(), gauge)? else {
        return Ok(Access::Locked);
    };
    if encrypted && password.is_empty() {
        return Ok(Access::Locked);
    }
    let folder = place.folder()?;
    let mut cursor = opened().open_for_processing()?;
    loop {
        let header = match cursor.read_header() {
            Ok(Some(header)) => header,
            Ok(None) => return Ok(Access::Open),
            Err(error) if refused(&error, encrypted) => return Ok(Access::Locked),
            Err(error) => return Err(error.into()),
        };
        progress.ensure_running()?;
        let entry = header.entry();
        let (sealed, size) = (entry.is_encrypted(), entry.unpacked_size);
        let next = match inside(&folder, &entry.filename.to_string_lossy()) {
            Some(target) if entry.is_directory() => {
                fs::create_dir_all(&target)?;
                header.skip()
            }
            Some(target) => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                header.extract_to(&target)
            }
            None => header.skip(),
        };
        cursor = match next {
            Ok(cursor) => cursor,
            Err(error) if refused(&error, sealed) => return Ok(Access::Locked),
            Err(error) => return Err(error.into()),
        };
        gauge.advance(size);
    }
}

fn survey(archive: unrar::Archive, gauge: &mut Gauge) -> Result<Option<bool>> {
    let listing = match archive.open_for_listing() {
        Ok(listing) => listing,
        Err(error) if refused(&error, false) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if listing.volume_info() == VolumeInfo::Subsequent {
        bail!(tr().unpack_volume);
    }
    let mut encrypted = false;
    for header in listing {
        let header = match header {
            Ok(header) => header,
            Err(error) if refused(&error, false) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        encrypted |= header.is_encrypted();
        gauge.total += header.unpacked_size;
    }
    Ok(Some(encrypted))
}

fn refused(error: &UnrarError, sealed: bool) -> bool {
    matches!(error.code, Code::MissingPassword | Code::BadPassword) || (sealed && error.code == Code::BadData)
}

#[cfg(test)]
mod tests;
