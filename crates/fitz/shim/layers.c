/* SPDX-License-Identifier: AGPL-3.0-or-later */
/*
 * Guarded wrappers for MuPDF calls that can throw and that mupdf-sys does not
 * wrap (ADR 0004). Every function runs inside fz_try and reports failures
 * through the same error out-pointer as the mupdf_* wrappers, so an
 * exception can never escape to Rust (MuPDF would exit the process).
 */

#include "internal.h"

int ofp_count_layer_ui(fz_context *ctx, fz_document *doc, mupdf_error_t **errptr)
{
    int n = 0;
    pdf_document *pdf = pdf_specifics(ctx, doc);
    if (!pdf)
        return 0;
    fz_try(ctx)
        n = pdf_count_layer_config_ui(ctx, pdf);
    fz_catch(ctx)
        mupdf_save_error(ctx, errptr);
    return n;
}

/* On success `*text` points into MuPDF-owned memory valid until the layer
 * configuration changes; copy it at once. */
void ofp_layer_ui_info(fz_context *ctx, fz_document *doc, int ui, pdf_layer_config_ui *info,
                       mupdf_error_t **errptr)
{
    pdf_document *pdf = pdf_specifics(ctx, doc);
    memset(info, 0, sizeof *info);
    if (!pdf)
        return;
    fz_try(ctx)
        pdf_layer_config_ui_info(ctx, pdf, ui, info);
    fz_catch(ctx)
        mupdf_save_error(ctx, errptr);
}

void ofp_set_layer_ui(fz_context *ctx, fz_document *doc, int ui, int on, mupdf_error_t **errptr)
{
    pdf_document *pdf = pdf_specifics(ctx, doc);
    if (!pdf)
        return;
    fz_try(ctx)
    {
        if (on)
            pdf_select_layer_config_ui(ctx, pdf, ui);
        else
            pdf_deselect_layer_config_ui(ctx, pdf, ui);
    }
    fz_catch(ctx)
        mupdf_save_error(ctx, errptr);
}
