use crate::i18n::tr;
use anyhow::{Result, anyhow};
use std::panic::{AssertUnwindSafe, catch_unwind};

pub fn failure() -> anyhow::Error {
    anyhow!(tr().internal_error)
}

pub fn catch<T>(job: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(job)).unwrap_or_else(|_| Err(failure()))
}

pub fn or_else<T>(fallback: impl FnOnce() -> T, job: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(job)).unwrap_or_else(|_| fallback())
}
