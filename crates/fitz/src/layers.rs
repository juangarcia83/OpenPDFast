// SPDX-License-Identifier: AGPL-3.0-or-later

//! Optional content (OCG layers), through the guarded C shim (ADR 0004).
//!
//! MuPDF exposes the document's layer configuration as a flat "UI list" in
//! display order: labels, checkboxes and radio buttons, each with a depth for
//! nesting. Toggling an entry changes how pages are interpreted, so display
//! lists built before the change are stale.

use std::ffi::{CStr, c_int};

use mupdf_sys::{
    PDF_LAYER_UI_CHECKBOX, PDF_LAYER_UI_RADIOBOX, fz_context, fz_document, mupdf_error_t,
    pdf_layer_config_ui,
};

use crate::context::ctx;
use crate::document::Document;
use crate::error::{Result, check};

unsafe extern "C" {
    fn ofp_count_layer_ui(
        ctx: *mut fz_context,
        doc: *mut fz_document,
        err: *mut *mut mupdf_error_t,
    ) -> c_int;
    fn ofp_layer_ui_info(
        ctx: *mut fz_context,
        doc: *mut fz_document,
        ui: c_int,
        info: *mut pdf_layer_config_ui,
        err: *mut *mut mupdf_error_t,
    );
    fn ofp_set_layer_ui(
        ctx: *mut fz_context,
        doc: *mut fz_document,
        ui: c_int,
        on: c_int,
        err: *mut *mut mupdf_error_t,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    /// A heading; cannot be toggled.
    Label,
    Checkbox,
    /// One of a group where selecting one deselects the others.
    Radio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// Position in the UI list; pass it back to [`Document::set_layer`].
    pub index: u32,
    pub name: String,
    pub depth: u32,
    pub kind: LayerKind,
    pub visible: bool,
    /// The document forbids changing this entry.
    pub locked: bool,
}

impl Document {
    /// The document's layers in display order (empty if it has none).
    pub fn layers(&self) -> Result<Vec<Layer>> {
        let ctx = ctx()?;
        let mut err = std::ptr::null_mut();
        // SAFETY: valid context and document; the shim catches exceptions.
        let n = unsafe { ofp_count_layer_ui(ctx, self.as_ptr(), &mut err) };
        check(err)?;
        let mut out = Vec::with_capacity(usize::try_from(n).unwrap_or(0));
        for i in 0..n.max(0) {
            let mut info = pdf_layer_config_ui {
                text: std::ptr::null(),
                depth: 0,
                type_: 0,
                selected: 0,
                locked: 0,
            };
            let mut err = std::ptr::null_mut();
            // SAFETY: `info` is a valid out-parameter; exceptions are caught.
            unsafe { ofp_layer_ui_info(ctx, self.as_ptr(), i, &mut info, &mut err) };
            check(err)?;
            let name = if info.text.is_null() {
                String::new()
            } else {
                // SAFETY: MuPDF returns a NUL-terminated string that stays
                // valid until the configuration changes; copied right away.
                unsafe { CStr::from_ptr(info.text) }
                    .to_string_lossy()
                    .into_owned()
            };
            let kind = match info.type_ {
                PDF_LAYER_UI_CHECKBOX => LayerKind::Checkbox,
                PDF_LAYER_UI_RADIOBOX => LayerKind::Radio,
                _ => LayerKind::Label,
            };
            out.push(Layer {
                index: u32::try_from(i).unwrap_or(0),
                name,
                depth: u32::try_from(info.depth).unwrap_or(0),
                kind,
                visible: info.selected != 0,
                locked: info.locked != 0,
            });
        }
        Ok(out)
    }

    /// Shows or hides a layer. Display lists of every page must be rebuilt
    /// afterwards.
    pub fn set_layer(&mut self, index: u32, visible: bool) -> Result<()> {
        let ctx = ctx()?;
        let ui = c_int::try_from(index).unwrap_or(c_int::MAX);
        let mut err = std::ptr::null_mut();
        // SAFETY: valid context and document; the shim catches exceptions.
        unsafe { ofp_set_layer_ui(ctx, self.as_ptr(), ui, c_int::from(visible), &mut err) };
        check(err)
    }
}
