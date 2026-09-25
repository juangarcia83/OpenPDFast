// SPDX-License-Identifier: AGPL-3.0-or-later

use mupdf_sys::{
    fz_bound_display_list, fz_close_device, fz_device, fz_display_list, fz_drop_device,
    fz_drop_display_list,
};

use crate::context::ctx;
use crate::cookie::Cookie;
use crate::error::{Result, ffi_try};
use crate::geometry::{IRect, Matrix, Rect};
use crate::pixmap::Pixmap;

/// A page interpreted once into MuPDF's display list: every tile and zoom
/// level is rasterized from it without re-parsing the page (ADR 0002).
pub struct DisplayList {
    ptr: *mut fz_display_list,
}

// SAFETY: a finished display list is immutable and MuPDF supports running it
// from several threads concurrently, each with its own cloned context. We
// never expose a way to record into a list after construction.
unsafe impl Send for DisplayList {}
// SAFETY: see above.
unsafe impl Sync for DisplayList {}

/// Closes and drops a device even on early return.
struct DeviceGuard(*mut fz_device);

impl Drop for DeviceGuard {
    fn drop(&mut self) {
        if let Ok(ctx) = ctx() {
            // SAFETY: we own the device. `fz_close_device` of draw and list
            // devices only flushes internal state; it does not throw in
            // practice (mupdf-sys has no wrapper for it).
            unsafe {
                fz_close_device(ctx, self.0);
                fz_drop_device(ctx, self.0);
            }
        }
    }
}

impl DisplayList {
    /// Takes ownership of a list returned by a MuPDF constructor.
    pub(crate) fn from_raw(ptr: *mut fz_display_list) -> Self {
        Self { ptr }
    }

    pub fn bounds(&self) -> Rect {
        // SAFETY: valid list; bounding cannot throw.
        ctx()
            .map(|c| unsafe { fz_bound_display_list(c, self.ptr) }.into())
            .unwrap_or_default()
    }

    /// Rasterizes the part of the page that falls into `tile` (device pixels)
    /// once transformed by `ctm`, over a white background.
    pub fn render(&self, ctm: Matrix, tile: IRect, cookie: Option<&Cookie>) -> Result<Pixmap> {
        let ctx = ctx()?;
        let pix = Pixmap::new_white_rgba(tile)?;
        let dev = DeviceGuard(ffi_try!(mupdf_new_draw_device(
            ctx,
            pix.as_ptr(),
            tile.into()
        ))?);
        let cookie = cookie.map_or(std::ptr::null_mut(), Cookie::as_ptr);
        ffi_try!(mupdf_display_list_run(
            ctx,
            self.ptr,
            dev.0,
            ctm.into(),
            Rect::from(tile).into(),
            cookie
        ))?;
        drop(dev);
        Ok(pix)
    }

    /// Records the subset of this list that intersects `area` (in page space)
    /// into a new list. Used to partition heavy pages spatially (ADR 0002).
    pub fn extract(&self, area: Rect) -> Result<DisplayList> {
        let ctx = ctx()?;
        let sub = DisplayList::from_raw(ffi_try!(mupdf_new_display_list(ctx, area.into()))?);
        let dev = DeviceGuard(ffi_try!(mupdf_new_display_list_device(ctx, sub.ptr))?);
        ffi_try!(mupdf_display_list_run(
            ctx,
            self.ptr,
            dev.0,
            Matrix::IDENTITY.into(),
            area.into(),
            std::ptr::null_mut()
        ))?;
        drop(dev);
        Ok(sub)
    }
}

impl Drop for DisplayList {
    fn drop(&mut self) {
        if let Ok(ctx) = ctx() {
            // SAFETY: we own one reference; dropping cannot throw.
            unsafe { fz_drop_display_list(ctx, self.ptr) };
        }
    }
}
