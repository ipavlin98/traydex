use crate::store::{self, Storage};
use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, LoggerHandle, Naming, detailed_format};
use std::{backtrace::Backtrace, path::PathBuf};

const FILE_LIMIT: u64 = 2 * 1024 * 1024;
const KEPT_FILES: usize = 4;

pub fn folder(storage: &Storage) -> PathBuf {
    storage.root().join("logs")
}

pub fn start() -> Option<LoggerHandle> {
    std::panic::set_hook(Box::new(|info| {
        log::error!("Panic: {info}\n{}", Backtrace::force_capture());
        log::logger().flush();
    }));
    let handle = Logger::try_with_env_or_str("info, traydex=debug, gpui=warn")
        .ok()?
        .log_to_file(
            FileSpec::default()
                .directory(folder(&Storage::new(store::root())))
                .basename("traydex")
                .suppress_timestamp(),
        )
        .rotate(Criterion::Size(FILE_LIMIT), Naming::Numbers, Cleanup::KeepLogFiles(KEPT_FILES))
        .append()
        .format_for_files(detailed_format)
        .start()
        .ok()?;
    match crate::profile::current().testing() {
        true => log::info!("{} started with the test profile in {}", crate::PRODUCT, store::root().display()),
        false => log::info!("{} started", crate::PRODUCT),
    }
    Some(handle)
}
