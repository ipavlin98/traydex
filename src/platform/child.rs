use crate::system::Progress;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows::core::PCWSTR;

pub(super) const NO_WINDOW: u32 = 0x0800_0000;
const POLL: Duration = Duration::from_millis(50);

pub struct Tether(HANDLE);

impl Drop for Tether {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub fn tether(child: &Child) -> Option<Tether> {
    let tied = unsafe {
        CreateJobObjectW(None, PCWSTR::null()).map(Tether).and_then(|tether| {
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                tether.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of_val(&limits) as u32,
            )?;
            AssignProcessToJobObject(tether.0, HANDLE(child.as_raw_handle()))?;
            Ok(tether)
        })
    };
    tied.inspect_err(|error| log::warn!("Couldn't tie a child process to the app: {error}")).ok()
}

fn start(command: &mut Command, input: &Option<Vec<u8>>) -> io::Result<Child> {
    let stdin = if input.is_some() { Stdio::piped() } else { Stdio::null() };
    command.stdin(stdin).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(NO_WINDOW).spawn()
}

pub fn output(command: &mut Command, input: Option<Vec<u8>>, progress: &Progress) -> io::Result<Output> {
    let mut child = start(command, &input)?;
    let tether = tether(&child);
    let result = watch(&mut child, input, progress);
    stop(&mut child, tether, result.is_err());
    result
}

pub fn stream(
    command: &mut Command,
    input: Option<Vec<u8>>,
    progress: &Progress,
    line: &mut dyn FnMut(&str),
) -> io::Result<Output> {
    let mut child = start(command, &input)?;
    let tether = tether(&child);
    let result = follow(&mut child, input, progress, line);
    stop(&mut child, tether, result.is_err());
    result
}

fn stop(child: &mut Child, tether: Option<Tether>, failed: bool) {
    if failed {
        drop(tether);
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn feed(child: &mut Child, input: Option<Vec<u8>>) -> io::Result<Option<JoinHandle<Vec<u8>>>> {
    match (child.stdin.take(), input) {
        (Some(mut stdin), Some(input)) => Some(spawn("traydex-input", move || {
            let _ = stdin.write_all(&input);
            Vec::new()
        }))
        .transpose(),
        _ => Ok(None),
    }
}

fn watch(child: &mut Child, input: Option<Vec<u8>>, progress: &Progress) -> io::Result<Output> {
    let writer = feed(child, input)?;
    let stdout = drain(child.stdout.take())?;
    let stderr = drain(child.stderr.take())?;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if progress.cancelled() {
            log::info!("Stopping a child process on request");
            return Err(io::ErrorKind::Interrupted.into());
        }
        thread::sleep(POLL);
    };
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    Ok(Output { status, stdout: stdout.join().unwrap_or_default(), stderr: stderr.join().unwrap_or_default() })
}

fn follow(
    child: &mut Child,
    input: Option<Vec<u8>>,
    progress: &Progress,
    line: &mut dyn FnMut(&str),
) -> io::Result<Output> {
    let writer = feed(child, input)?;
    let (tx, lines) = mpsc::channel::<String>();
    let reader = child.stdout.take().map(|stdout| {
        thread::Builder::new().name("traydex-lines".into()).spawn(move || {
            for text in BufReader::new(stdout).split(b'\n').map_while(Result::ok) {
                if tx.send(String::from_utf8_lossy(&text).trim_end().to_string()).is_err() {
                    break;
                }
            }
        })
    });
    let reader = reader.transpose()?;
    let stderr = drain(child.stderr.take())?;
    let mut stdout = Vec::new();
    loop {
        if progress.cancelled() {
            log::info!("Stopping a child process on request");
            return Err(io::ErrorKind::Interrupted.into());
        }
        match lines.recv_timeout(POLL) {
            Ok(text) => {
                line(&text);
                stdout.extend_from_slice(text.as_bytes());
                stdout.push(b'\n');
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait()?;
    if let Some(reader) = reader {
        let _ = reader.join();
    }
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    Ok(Output { status, stdout, stderr: stderr.join().unwrap_or_default() })
}

fn spawn(name: &str, work: impl FnOnce() -> Vec<u8> + Send + 'static) -> io::Result<JoinHandle<Vec<u8>>> {
    thread::Builder::new().name(name.into()).spawn(work)
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> io::Result<JoinHandle<Vec<u8>>> {
    spawn("traydex-output", move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}
