use super::upload::{base64, mime};
use super::{Outcome, Progress, base, extension, normalized, output};
use crate::i18n::{fill, tr};
use crate::os;
use crate::store::{Storage, human_size};
use crate::system::Services;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

const DATA_URI_LIMIT: u64 = 10 * 1024 * 1024;

pub(super) fn copy_paths(files: &[PathBuf]) -> Result<Outcome> {
    let text = files.iter().map(|file| file.display().to_string()).collect::<Vec<_>>().join("\r\n");
    let message = match files.len() {
        1 => tr().path_copied.to_string(),
        count => fill(tr().paths_copied, count),
    };
    Ok(Outcome { message, created: Vec::new(), clipboard: Some(text), show: None })
}

pub(super) fn data_uri(files: &[PathBuf]) -> Result<Outcome> {
    let files: Vec<&PathBuf> = files.iter().filter(|file| file.is_file()).collect();
    if files.is_empty() {
        bail!(tr().needs_file_input);
    }
    let mut uris = Vec::new();
    for file in &files {
        if fs::metadata(file)?.len() > DATA_URI_LIMIT {
            bail!(fill(tr().base64_too_big, base(file)));
        }
        let kind = mime(file).replace(' ', "");
        uris.push(format!("data:{kind};base64,{}", base64(&fs::read(file)?)));
    }
    let text = uris.join("\r\n");
    let message = match files.len() {
        1 => fill(tr().base64_copied, human_size(text.len() as u64)),
        count => fill(tr().base64_many, count),
    };
    Ok(Outcome { message, created: Vec::new(), clipboard: Some(text), show: None })
}

pub(super) fn transfer(
    files: &[PathBuf],
    folder: &Path,
    copy: bool,
    title: &str,
    system: &dyn Services,
) -> Result<Outcome> {
    if !folder.is_dir() {
        bail!(fill(tr().folder_missing, folder.display()));
    }
    let target = normalized(folder);
    let files: Vec<PathBuf> =
        files.iter().filter(|file| file.parent().map(normalized).as_ref() != Some(&target)).cloned().collect();
    if files.is_empty() {
        bail!(tr().already_there);
    }
    let nested = files.iter().any(|file| {
        let source = normalized(file);
        target == source || target.starts_with(&format!("{source}\\"))
    });
    if nested {
        bail!(tr().inside_itself);
    }
    system.transfer(&files, folder, copy)?;
    let message = if copy { tr().copied_to } else { tr().moved_to };
    Ok(Outcome::note(fill(message, title)))
}

pub(super) fn zip(files: &[PathBuf], progress: &Progress, storage: &Storage) -> Result<Outcome> {
    let first = files.first().context(tr().unsupported)?;
    let name = if files.len() > 1 {
        tr().archive.to_string()
    } else if first.is_dir() {
        base(first)
    } else {
        first.file_stem().unwrap_or_default().to_string_lossy().into_owned()
    };
    let target = output(first, &format!("{name}.zip"), storage)?;
    if let Err(error) = write_zip(&target, files, progress) {
        storage.discard(&target);
        return Err(error);
    }
    let path = target.display().to_string();
    Ok(Outcome { message: tr().zipped.into(), created: vec![target], clipboard: Some(path), show: None })
}

fn write_zip(target: &Path, files: &[PathBuf], progress: &Progress) -> Result<()> {
    let mut writer = ZipWriter::new(fs::File::create(target)?);
    let mut meter = Meter { progress, done: 0, total: files.iter().map(|file| size(file)).sum() };
    for file in files {
        pack(&mut writer, file, &base(file), &mut meter)?;
    }
    writer.finish()?;
    Ok(())
}

fn size(path: &Path) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            fs::read_dir(path).into_iter().flatten().flatten().map(|child| size(&child.path())).sum()
        }
        Ok(metadata) if !metadata.is_symlink() => metadata.len(),
        _ => 0,
    }
}

struct Meter<'a> {
    progress: &'a Progress,
    done: u64,
    total: u64,
}

struct Metered<'a, 'b, R> {
    inner: R,
    meter: &'b mut Meter<'a>,
}

impl<R: io::Read> io::Read for Metered<'_, '_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.meter.done += read as u64;
        self.meter.progress.fraction(self.meter.done, self.meter.total);
        Ok(read)
    }
}

fn pack(writer: &mut ZipWriter<fs::File>, path: &Path, entry: &str, meter: &mut Meter) -> Result<()> {
    let progress = meter.progress;
    if progress.cancelled() {
        bail!(tr().stopped);
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_symlink() {
        return Ok(());
    }
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(modified(&metadata))
        .large_file(metadata.len() >= u32::MAX as u64);
    if metadata.is_dir() {
        writer.add_directory(entry, options)?;
        for child in fs::read_dir(path)? {
            let child = child?;
            let name = format!("{entry}/{}", child.file_name().to_string_lossy());
            pack(writer, &child.path(), &name, meter)?;
        }
    } else {
        writer.start_file(entry, options)?;
        io::copy(&mut Metered { inner: progress.guard(fs::File::open(path)?), meter }, writer)?;
    }
    Ok(())
}

pub(super) fn modified(metadata: &fs::Metadata) -> DateTime {
    os::local_dos_time(metadata).and_then(|stamp| DateTime::try_from(stamp).ok()).unwrap_or_default()
}

pub(super) fn launch(program: &Path, files: &[PathBuf], title: &str, system: &dyn Services) -> Result<Outcome> {
    if !program.is_file() {
        bail!(fill(tr().program_missing, program.display()));
    }
    let mut command = if extension(program) == "ps1" {
        let mut shell = Command::new("powershell.exe");
        shell.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(program);
        shell
    } else {
        Command::new(program)
    };
    command.args(files);
    if let Some(folder) = files.first().and_then(|file| file.parent()) {
        command.current_dir(folder);
    }
    system.launch(&mut command)?;
    Ok(Outcome::note(fill(tr().launched, title)))
}
