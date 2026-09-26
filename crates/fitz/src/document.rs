// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::{CStr, CString};
use std::path::Path;

use mupdf_sys::{
    PDF_ENUM_NAME_CropBox, PDF_ENUM_NAME_MediaBox, PDF_ENUM_NAME_Rotate, PDF_ENUM_NAME_UserUnit,
    fz_document, fz_drop_document, fz_drop_page, fz_page, mupdf_drop_str, pdf_obj,
    pdf_set_page_tree_cache, pdf_specifics, pdf_to_int, pdf_to_real, pdf_to_rect,
};

use crate::context::ctx;
use crate::display_list::DisplayList;
use crate::error::{Error, Result, ffi_try};
use crate::geometry::Rect;

/// An open document (PDF, or anything else MuPDF recognizes).
///
/// MuPDF documents are not thread-safe: a `Document` can move to another
/// thread but must not be used from two threads at once (hence `!Sync`).
pub struct Document {
    ptr: *mut fz_document,
}

// SAFETY: MuPDF allows using a document from any thread as long as accesses
// are not concurrent; `Document` is `!Sync`, so `&mut`/ownership enforce that.
unsafe impl Send for Document {}

/// Standard metadata keys (`fz_lookup_metadata`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataKey {
    Format,
    Encryption,
    Title,
    Author,
    Subject,
    Keywords,
    Creator,
    Producer,
    CreationDate,
    ModDate,
}

impl MetadataKey {
    fn as_cstr(self) -> &'static CStr {
        match self {
            Self::Format => c"format",
            Self::Encryption => c"encryption",
            Self::Title => c"info:Title",
            Self::Author => c"info:Author",
            Self::Subject => c"info:Subject",
            Self::Keywords => c"info:Keywords",
            Self::Creator => c"info:Creator",
            Self::Producer => c"info:Producer",
            Self::CreationDate => c"info:CreationDate",
            Self::ModDate => c"info:ModDate",
        }
    }
}

impl Document {
    /// Opens a file. Only the header and cross-reference data are read;
    /// page contents are interpreted lazily.
    pub fn open(path: &Path) -> Result<Self> {
        let path = path
            .to_str()
            .ok_or(Error::InvalidArgument("path is not valid UTF-8"))?;
        let path = CString::new(path).map_err(|_| Error::InvalidArgument("path contains NUL"))?;
        let ctx = ctx()?;
        let ptr = ffi_try!(mupdf_open_document(ctx, path.as_ptr()))?;
        if ptr.is_null() {
            return Err(Error::InvalidArgument("unrecognized document"));
        }
        Ok(Self { ptr })
    }

    pub fn needs_password(&self) -> Result<bool> {
        ffi_try!(mupdf_needs_password(ctx()?, self.ptr))
    }

    /// Returns `true` if the password unlocked the document.
    pub fn authenticate(&mut self, password: &str) -> Result<bool> {
        let password =
            CString::new(password).map_err(|_| Error::InvalidArgument("password contains NUL"))?;
        ffi_try!(mupdf_authenticate_password(
            ctx()?,
            self.ptr,
            password.as_ptr()
        ))
    }

    pub fn page_count(&self) -> Result<usize> {
        let n = ffi_try!(mupdf_document_page_count(ctx()?, self.ptr))?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    /// Returns the metadata value, or `None` when absent or empty.
    pub fn metadata(&self, key: MetadataKey) -> Result<Option<String>> {
        let raw = ffi_try!(mupdf_lookup_metadata(
            ctx()?,
            self.ptr,
            key.as_cstr().as_ptr()
        ))?;
        if raw.is_null() {
            return Ok(None);
        }
        // SAFETY: the wrapper returns a calloc'ed NUL-terminated string that we
        // own and release with `mupdf_drop_str`.
        let value = unsafe {
            let s = CStr::from_ptr(raw).to_string_lossy().into_owned();
            mupdf_drop_str(raw);
            s
        };
        Ok((!value.is_empty()).then_some(value))
    }

    /// Page size in points (rotation and `/UserUnit` applied), read from the
    /// PDF page tree without loading the page. Loading a page also parses its
    /// annotations and links, which dominates opening time on documents with
    /// many links (e.g. 328 ms for the 1324-page PGF manual).
    ///
    /// Mirrors MuPDF's `pdf_page_obj_transform_box` so sizes match
    /// [`Page::bounds`] exactly. Returns `Ok(None)` for non-PDF documents.
    pub fn page_size_fast(&self, index: usize) -> Result<Option<(f32, f32)>> {
        let ctx = ctx()?;
        // SAFETY: plain type check on a valid document; cannot throw.
        let pdf = unsafe { pdf_specifics(ctx, self.ptr) };
        if pdf.is_null() {
            return Ok(None);
        }
        let index = i32::try_from(index).map_err(|_| Error::InvalidArgument("page index"))?;
        let page = ffi_try!(mupdf_pdf_lookup_page_obj(ctx, pdf, index))?;
        if page.is_null() {
            return Err(Error::InvalidArgument("page not found"));
        }
        // PDF_NAME(X) in C is the enum value cast to a pointer. The enum's
        // Rust type is i32 on MSVC and u32 elsewhere, hence `as isize` at call sites.
        let name = |n: isize| n as *mut pdf_obj;
        let inherited = |n: isize| ffi_try!(mupdf_pdf_dict_get_inheritable(ctx, page, name(n)));
        let user_unit = ffi_try!(mupdf_pdf_dict_get(
            ctx,
            page,
            name(PDF_ENUM_NAME_UserUnit as isize)
        ))?;
        let media = inherited(PDF_ENUM_NAME_MediaBox as isize)?;
        let crop = inherited(PDF_ENUM_NAME_CropBox as isize)?;
        let rotate = inherited(PDF_ENUM_NAME_Rotate as isize)?;
        // SAFETY: pdf_to_* accessors accept null and never throw.
        let (user_unit, media, crop, rotate) = unsafe {
            let uu = if user_unit.is_null() {
                1.0
            } else {
                pdf_to_real(ctx, user_unit)
            };
            let crop = (!crop.is_null()).then(|| Rect::from(pdf_to_rect(ctx, crop)));
            (
                uu,
                Rect::from(pdf_to_rect(ctx, media)),
                crop,
                pdf_to_int(ctx, rotate),
            )
        };
        let mut used = match crop {
            Some(c) => intersect(media, c),
            None => media,
        };
        if used.is_empty() {
            used = Rect::new(0.0, 0.0, 612.0, 792.0);
        }
        let (mut w, mut h) = (used.width().abs(), used.height().abs());
        if w < 1.0 || h < 1.0 {
            (w, h) = (1.0, 1.0);
        }
        let rotate = snap_rotation(rotate);
        let (w, h) = (w * user_unit, h * user_unit);
        Ok(Some(if rotate == 90 || rotate == 270 {
            (h, w)
        } else {
            (w, h)
        }))
    }

    /// Enables or disables MuPDF's page-tree map for PDFs (no-op otherwise).
    ///
    /// With the map (the default) the first page lookup loads *every* page
    /// object, e.g. 263 ms for the 1324-page PGF manual whose page objects
    /// sit in 651 object streams. Without it, lookups walk the tree lazily.
    pub fn set_page_tree_cache(&mut self, enabled: bool) -> Result<()> {
        let ctx = ctx()?;
        // SAFETY: plain type check on a valid document; cannot throw.
        let pdf = unsafe { pdf_specifics(ctx, self.ptr) };
        if !pdf.is_null() {
            // SAFETY: valid PDF document. Disabling only frees the map and
            // enabling only sets a flag; neither can throw.
            unsafe { pdf_set_page_tree_cache(ctx, pdf, i32::from(enabled)) };
        }
        Ok(())
    }

    pub fn load_page(&self, index: usize) -> Result<Page> {
        let index = i32::try_from(index).map_err(|_| Error::InvalidArgument("page index"))?;
        let ptr = ffi_try!(mupdf_load_page(ctx()?, self.ptr, index))?;
        if ptr.is_null() {
            return Err(Error::InvalidArgument("page not found"));
        }
        Ok(Page { ptr })
    }
}

fn intersect(a: Rect, b: Rect) -> Rect {
    // MuPDF normalizes neither box before intersecting; mimic fz_intersect_rect.
    Rect::new(
        a.x0.max(b.x0),
        a.y0.max(b.y0),
        a.x1.min(b.x1),
        a.y1.min(b.y1),
    )
}

/// Snaps `/Rotate` to 0, 90, 180 or 270 like MuPDF.
fn snap_rotation(rotate: i32) -> i32 {
    let mut r = rotate;
    if r < 0 {
        r = 360 - (r.unsigned_abs() % 360) as i32;
    }
    if r >= 360 {
        r %= 360;
    }
    r = 90 * ((r + 45) / 90);
    if r >= 360 { 0 } else { r }
}

impl Drop for Document {
    fn drop(&mut self) {
        if let Ok(ctx) = ctx() {
            // SAFETY: we own one reference; dropping cannot throw.
            unsafe { fz_drop_document(ctx, self.ptr) };
        }
    }
}

/// A loaded page. Holds its own reference to the document internally.
pub struct Page {
    ptr: *mut fz_page,
}

// SAFETY: same rules as `Document`: movable, never used concurrently.
unsafe impl Send for Page {}

impl Page {
    /// Page box in points, after `/Rotate` and `/UserUnit` are applied.
    pub fn bounds(&self) -> Result<Rect> {
        Ok(ffi_try!(mupdf_bound_page(ctx()?, self.ptr))?.into())
    }

    /// Interprets the page content (and annotations) into a display list.
    pub fn to_display_list(&self, annotations: bool) -> Result<DisplayList> {
        let ptr = ffi_try!(mupdf_page_to_display_list(ctx()?, self.ptr, annotations))?;
        if ptr.is_null() {
            return Err(Error::InvalidArgument("empty display list"));
        }
        Ok(DisplayList::from_raw(ptr))
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        if let Ok(ctx) = ctx() {
            // SAFETY: we own one reference; dropping cannot throw.
            unsafe { fz_drop_page(ctx, self.ptr) };
        }
    }
}
