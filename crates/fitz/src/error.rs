// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::CStr;

use mupdf_sys::{mupdf_drop_error, mupdf_error_t};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Broad category of a MuPDF failure, mapped from `FZ_ERROR_*` codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Out of memory or other system failure.
    System,
    /// Malformed or unsupported document data.
    Format,
    /// Wrong argument or state (a bug on our side, or a hostile file).
    Argument,
    /// The operation was aborted through a [`crate::Cookie`].
    Aborted,
    /// Anything else MuPDF reports.
    Other,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("MuPDF error ({kind:?}): {message}")]
    Mupdf { kind: ErrorKind, message: String },
    #[error("MuPDF context unavailable: {0}")]
    Context(&'static str),
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
}

impl Error {
    pub(crate) fn context(msg: &'static str) -> Self {
        Self::Context(msg)
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Mupdf { kind, .. } => *kind,
            Self::Context(_) => ErrorKind::System,
            Self::InvalidArgument(_) => ErrorKind::Argument,
        }
    }
}

// FZ_ERROR_* codes from mupdf/fitz/context.h (1.27).
const FZ_ERROR_SYSTEM: i32 = 2;
const FZ_ERROR_LIBRARY: i32 = 3;
const FZ_ERROR_ARGUMENT: i32 = 4;
const FZ_ERROR_LIMIT: i32 = 5;
const FZ_ERROR_UNSUPPORTED: i32 = 6;
const FZ_ERROR_FORMAT: i32 = 7;
const FZ_ERROR_SYNTAX: i32 = 8;
const FZ_ERROR_ABORT: i32 = 10;

/// Converts the out-pointer error of a `mupdf_*` wrapper into a `Result`,
/// freeing the MuPDF error object.
pub(crate) fn check(err: *mut mupdf_error_t) -> Result<()> {
    if err.is_null() {
        return Ok(());
    }
    // SAFETY: a non-null error was allocated by a mupdf-sys wrapper; its
    // message is a valid C string (or null) until `mupdf_drop_error`.
    let (code, message) = unsafe {
        let e = &*err;
        let message = if e.message.is_null() {
            String::new()
        } else {
            CStr::from_ptr(e.message).to_string_lossy().into_owned()
        };
        (e.type_, message)
    };
    // SAFETY: `err` is owned by us and dropped exactly once.
    unsafe { mupdf_drop_error(err) };
    let kind = match code {
        FZ_ERROR_SYSTEM | FZ_ERROR_LIMIT => ErrorKind::System,
        FZ_ERROR_FORMAT | FZ_ERROR_SYNTAX | FZ_ERROR_UNSUPPORTED => ErrorKind::Format,
        FZ_ERROR_ARGUMENT => ErrorKind::Argument,
        FZ_ERROR_ABORT => ErrorKind::Aborted,
        FZ_ERROR_LIBRARY => ErrorKind::Other,
        _ => ErrorKind::Other,
    };
    Err(Error::Mupdf { kind, message })
}

/// Calls a `mupdf_*` wrapper with a fresh error out-pointer and checks it.
macro_rules! ffi_try {
    ($func:ident ( $($arg:expr),* $(,)? )) => {{
        let mut err: *mut mupdf_sys::mupdf_error_t = std::ptr::null_mut();
        #[allow(unused_unsafe)]
        // SAFETY: the caller passes valid pointers; the wrapper catches MuPDF
        // exceptions and reports them through `err`.
        let value = unsafe { mupdf_sys::$func($($arg,)* &mut err) };
        $crate::error::check(err).map(|()| value)
    }};
}

pub(crate) use ffi_try;
