/* SPDX-License-Identifier: AGPL-3.0-or-later */
/*
 * Force-included by bindgen when building mupdf-sys for MSVC targets.
 *
 * The `mupdf` crate uses a `max_align_t` binding, but MSVC headers never
 * declare it in C, and bindgen drops clang's `typedef double max_align_t`
 * once mupdf-sys marks it opaque. A struct with the same size and
 * alignment (8 bytes, like MSVC's double) is emitted correctly.
 */
#ifndef __CLANG_MAX_ALIGN_T_DEFINED
#define __CLANG_MAX_ALIGN_T_DEFINED
typedef struct { double __max_align_d; } max_align_t;
#endif
