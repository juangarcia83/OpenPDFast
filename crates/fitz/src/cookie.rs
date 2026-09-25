// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicI32, Ordering};

use mupdf_sys::fz_cookie;

/// Progress/abort channel between a render running on one thread and a
/// controller on another. Share it with `Arc<Cookie>`.
pub struct Cookie {
    inner: Box<UnsafeCell<fz_cookie>>,
}

// SAFETY: MuPDF polls `abort` without locking by design; we only ever write
// that field, through an atomic view with the same size and alignment as the
// C `int`. Other fields are written by MuPDF on the rendering thread only.
unsafe impl Send for Cookie {}
// SAFETY: see above.
unsafe impl Sync for Cookie {}

impl Cookie {
    pub fn new() -> Self {
        Self {
            inner: Box::new(UnsafeCell::new(fz_cookie {
                abort: 0,
                progress: 0,
                progress_max: 0,
                errors: 0,
                incomplete: 0,
            })),
        }
    }

    fn abort_flag(&self) -> &AtomicI32 {
        // SAFETY: `abort` is a properly aligned `c_int` (== i32) living as long
        // as `self`; all our accesses to it are atomic.
        unsafe { AtomicI32::from_ptr(&raw mut (*self.inner.get()).abort) }
    }

    /// Asks any render using this cookie to stop as soon as possible.
    pub fn abort(&self) {
        self.abort_flag().store(1, Ordering::Relaxed);
    }

    pub fn is_aborted(&self) -> bool {
        self.abort_flag().load(Ordering::Relaxed) != 0
    }

    /// Pointer handed to MuPDF. Valid for as long as `self` lives.
    pub(crate) fn as_ptr(&self) -> *mut fz_cookie {
        self.inner.get()
    }
}

impl Default for Cookie {
    fn default() -> Self {
        Self::new()
    }
}
