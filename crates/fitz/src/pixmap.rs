// SPDX-License-Identifier: AGPL-3.0-or-later

use mupdf_sys::{
    fz_device_rgb, fz_drop_pixmap, fz_pixmap, fz_pixmap_components, fz_pixmap_height,
    fz_pixmap_samples, fz_pixmap_stride, fz_pixmap_width,
};

use crate::context::ctx;
use crate::error::{Result, ffi_try};
use crate::geometry::IRect;

/// RGBA pixmap (8 bits per channel, alpha always 255 once cleared to white).
pub struct Pixmap {
    ptr: *mut fz_pixmap,
}

// SAFETY: a pixmap is plain memory owned by this wrapper; MuPDF allows
// dropping it from any thread with that thread's context.
unsafe impl Send for Pixmap {}

impl Pixmap {
    /// Allocates an RGBA pixmap covering `rect` in device space, filled white.
    pub fn new_white_rgba(rect: IRect) -> Result<Self> {
        let ctx = ctx()?;
        // SAFETY: `fz_device_rgb` returns a static colorspace and cannot throw.
        let rgb = unsafe { fz_device_rgb(ctx) };
        let ptr = ffi_try!(mupdf_new_pixmap(
            ctx,
            rgb,
            rect.x0,
            rect.y0,
            rect.width(),
            rect.height(),
            true
        ))?;
        let pix = Self { ptr };
        ffi_try!(mupdf_clear_pixmap_with_value(ctx, ptr, 255))?;
        Ok(pix)
    }

    pub(crate) fn as_ptr(&self) -> *mut fz_pixmap {
        self.ptr
    }

    pub fn width(&self) -> u32 {
        // SAFETY: valid pixmap; plain field accessor, cannot throw.
        ctx()
            .map(|c| unsafe { fz_pixmap_width(c, self.ptr) } as u32)
            .unwrap_or(0)
    }

    pub fn height(&self) -> u32 {
        // SAFETY: as above.
        ctx()
            .map(|c| unsafe { fz_pixmap_height(c, self.ptr) } as u32)
            .unwrap_or(0)
    }

    /// Copies the samples out as tightly packed RGBA rows.
    pub fn to_rgba(&self) -> Result<Vec<u8>> {
        let ctx = ctx()?;
        // SAFETY: valid pixmap; accessors cannot throw. The samples buffer has
        // `stride * height` bytes and lives as long as the pixmap.
        unsafe {
            let (w, h) = (
                fz_pixmap_width(ctx, self.ptr),
                fz_pixmap_height(ctx, self.ptr),
            );
            let n = fz_pixmap_components(ctx, self.ptr);
            let stride = fz_pixmap_stride(ctx, self.ptr) as usize;
            let row = (w * n) as usize;
            let samples = fz_pixmap_samples(ctx, self.ptr);
            if samples.is_null() || w <= 0 || h <= 0 {
                return Ok(Vec::new());
            }
            let data = std::slice::from_raw_parts(samples, stride * h as usize);
            if stride == row {
                return Ok(data.to_vec());
            }
            let mut out = Vec::with_capacity(row * h as usize);
            for y in 0..h as usize {
                out.extend_from_slice(&data[y * stride..y * stride + row]);
            }
            Ok(out)
        }
    }
}

impl Drop for Pixmap {
    fn drop(&mut self) {
        if let Ok(ctx) = ctx() {
            // SAFETY: we own one reference to the pixmap; dropping cannot throw.
            unsafe { fz_drop_pixmap(ctx, self.ptr) };
        }
    }
}
