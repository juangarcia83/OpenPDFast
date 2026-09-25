// SPDX-License-Identifier: AGPL-3.0-or-later

//! Per-thread MuPDF contexts cloned from a single process-wide base context.

use std::cell::Cell;
use std::sync::OnceLock;

use std::ffi::{CStr, c_char, c_void};

use mupdf_sys::{
    fz_clone_context, fz_context, fz_drop_context, fz_set_error_callback, fz_set_warning_callback,
    mupdf_new_base_context,
};

use crate::error::{Error, Result};

/// The base context owns the locks, the resource store and the glyph cache
/// shared by every thread. It lives for the whole process.
struct Base(*mut fz_context);

// SAFETY: the base context is only used as the source of `fz_clone_context`,
// which MuPDF explicitly supports from any thread (it takes the context locks).
unsafe impl Send for Base {}
// SAFETY: see above; no other operation is performed on the base context.
unsafe impl Sync for Base {}

static BASE: OnceLock<Option<Base>> = OnceLock::new();

/// Owns this thread's cloned context and drops it when the thread exits.
struct Local(Cell<*mut fz_context>);

impl Drop for Local {
    fn drop(&mut self) {
        let ctx = self.0.get();
        if !ctx.is_null() {
            // SAFETY: `ctx` was cloned for this thread and is no longer used:
            // thread-local destructors run after all code on the thread.
            unsafe { fz_drop_context(ctx) };
        }
    }
}

thread_local! {
    static LOCAL: Local = const { Local(Cell::new(std::ptr::null_mut())) };
}

/// Forwards MuPDF errors to `tracing` (mupdf-sys silences them by default,
/// which also hides the message printed before an uncaught-exception exit).
unsafe extern "C" fn on_error(_user: *mut c_void, message: *const c_char) {
    if !message.is_null() {
        // SAFETY: MuPDF passes a valid NUL-terminated string for this call.
        let msg = unsafe { CStr::from_ptr(message) }.to_string_lossy();
        tracing::warn!(target: "mupdf", "{msg}");
    }
}

unsafe extern "C" fn on_warning(_user: *mut c_void, message: *const c_char) {
    if !message.is_null() {
        // SAFETY: as above.
        let msg = unsafe { CStr::from_ptr(message) }.to_string_lossy();
        tracing::debug!(target: "mupdf", "{msg}");
    }
}

/// Installs the logging callbacks on a context. Cannot throw.
fn install_callbacks(ctx: *mut fz_context) {
    // SAFETY: valid context; the callbacks never throw.
    unsafe {
        fz_set_error_callback(ctx, Some(on_error), std::ptr::null_mut());
        fz_set_warning_callback(ctx, Some(on_warning), std::ptr::null_mut());
    }
}

/// Returns this thread's MuPDF context, creating it on first use.
pub(crate) fn ctx() -> Result<*mut fz_context> {
    LOCAL.with(|local| {
        let existing = local.0.get();
        if !existing.is_null() {
            return Ok(existing);
        }
        let base = BASE
            .get_or_init(|| {
                // SAFETY: creates the base context with real locks and the
                // default allocator; called exactly once per process.
                let ptr = unsafe { mupdf_new_base_context() };
                if !ptr.is_null() {
                    install_callbacks(ptr);
                }
                (!ptr.is_null()).then_some(Base(ptr))
            })
            .as_ref()
            .ok_or_else(|| Error::context("failed to create the MuPDF base context"))?;
        // SAFETY: cloning a valid base context; the clone shares its locks.
        let ctx = unsafe { fz_clone_context(base.0) };
        if ctx.is_null() {
            return Err(Error::context("failed to clone the MuPDF context"));
        }
        install_callbacks(ctx);
        local.0.set(ctx);
        Ok(ctx)
    })
}
