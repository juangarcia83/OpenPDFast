// SPDX-License-Identifier: AGPL-3.0-or-later

//! Spike S1b: same experiment as S1, but through raw `mupdf-sys` with our own
//! `fz_context`, so we control the lock implementation and the allocator.
//!
//! MuPDF takes the global `FZ_LOCK_ALLOC` around *every* allocation and
//! every refcount change. S1 showed that with mupdf-rs defaults
//! (CRITICAL_SECTION + system malloc) rendering stops scaling past 2-4
//! threads. This binary measures lock contention per lock and compares
//! allocators.
//!
//! Usage: s1b-raw-context <file.pdf> <page> <system|mimalloc> [scale...]

use std::ffi::{CStr, CString, c_int, c_void};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ptr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::time::{Duration, Instant};

use mupdf_sys::*;
use parking_lot::RawMutex;
use parking_lot::lock_api::RawMutex as _;

const TILE: i32 = 512;
const LOCK_NAMES: [&str; 3] = ["ALLOC", "FREETYPE", "GLYPHCACHE"];

struct Locks {
    mutex: [RawMutex; 3],
    acquired: [AtomicU64; 3],
    contended: [AtomicU64; 3],
}

static LOCKS: Locks = Locks {
    mutex: [RawMutex::INIT, RawMutex::INIT, RawMutex::INIT],
    acquired: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
    contended: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
};

unsafe extern "C" fn lock(_user: *mut c_void, lock: c_int) {
    let i = lock as usize;
    LOCKS.acquired[i].fetch_add(1, Relaxed);
    if !LOCKS.mutex[i].try_lock() {
        LOCKS.contended[i].fetch_add(1, Relaxed);
        LOCKS.mutex[i].lock();
    }
}

unsafe extern "C" fn unlock(_user: *mut c_void, lock: c_int) {
    // SAFETY: MuPDF only unlocks locks it previously took on this thread.
    unsafe { LOCKS.mutex[lock as usize].unlock() };
}

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn realloc(p: *mut c_void, size: usize) -> *mut c_void;
    fn free(p: *mut c_void);
}

unsafe extern "C" fn sys_malloc(_: *mut c_void, size: usize) -> *mut c_void {
    unsafe { malloc(size) }
}
unsafe extern "C" fn sys_realloc(_: *mut c_void, p: *mut c_void, size: usize) -> *mut c_void {
    unsafe { realloc(p, size) }
}
unsafe extern "C" fn sys_free(_: *mut c_void, p: *mut c_void) {
    unsafe { free(p) }
}
unsafe extern "C" fn mi_malloc(_: *mut c_void, size: usize) -> *mut c_void {
    unsafe { libmimalloc_sys::mi_malloc(size) }
}
unsafe extern "C" fn mi_realloc(_: *mut c_void, p: *mut c_void, size: usize) -> *mut c_void {
    unsafe { libmimalloc_sys::mi_realloc(p, size) }
}
unsafe extern "C" fn mi_free(_: *mut c_void, p: *mut c_void) {
    unsafe { libmimalloc_sys::mi_free(p) }
}

fn check(err: *mut mupdf_error_t) -> Result<(), String> {
    if err.is_null() {
        return Ok(());
    }
    // SAFETY: a non-null error comes from a mupdf-sys wrapper and owns its message.
    unsafe {
        let msg = CStr::from_ptr((*err).message)
            .to_string_lossy()
            .into_owned();
        mupdf_drop_error(err);
        Err(msg)
    }
}

#[derive(Clone, Copy)]
struct Ctx(*mut fz_context);
// SAFETY: only used to clone per-thread contexts, which MuPDF allows.
unsafe impl Send for Ctx {}
unsafe impl Sync for Ctx {}

#[derive(Clone, Copy)]
struct List(*mut fz_display_list);
// SAFETY: a finished display list may be run from several threads at once.
unsafe impl Send for List {}
unsafe impl Sync for List {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path = CString::new(
        args.get(1)
            .ok_or("usage: <pdf> <page> <system|mimalloc> [scale...]")?
            .as_str(),
    )?;
    let page_no: c_int = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(0);
    let allocator = args.get(3).map(String::as_str).unwrap_or("system");
    let scales: Vec<f32> = args
        .iter()
        .skip(4)
        .map(|s| s.parse())
        .collect::<Result<_, _>>()?;
    let scales = if scales.is_empty() {
        vec![1.0, 2.0]
    } else {
        scales
    };

    let alloc = match allocator {
        "mimalloc" => fz_alloc_context {
            user: ptr::null_mut(),
            malloc: Some(mi_malloc),
            realloc: Some(mi_realloc),
            free: Some(mi_free),
        },
        _ => fz_alloc_context {
            user: ptr::null_mut(),
            malloc: Some(sys_malloc),
            realloc: Some(sys_realloc),
            free: Some(sys_free),
        },
    };
    let locks = fz_locks_context {
        user: ptr::null_mut(),
        lock: Some(lock),
        unlock: Some(unlock),
    };

    // SAFETY: plain FFI calls; every pointer is checked before use and all
    // throwing operations go through the mupdf_* wrappers (fz_try inside).
    unsafe {
        let base = fz_new_context_imp(&alloc, &locks, 256 << 20, c"1.27.2".as_ptr());
        if base.is_null() {
            return Err("fz_new_context failed (version mismatch?)".into());
        }
        fz_register_document_handlers(base);

        let mut err = ptr::null_mut();
        let doc = mupdf_open_document(base, path.as_ptr(), &mut err);
        check(err)?;
        let page = mupdf_load_page(base, doc, page_no, &mut err);
        check(err)?;
        let bounds = mupdf_bound_page(base, page, &mut err);
        check(err)?;
        let t = Instant::now();
        let list = mupdf_page_to_display_list(base, page, true, &mut err);
        check(err)?;
        println!(
            "allocator: {allocator} | locks: parking_lot | display list {}",
            ms(t.elapsed())
        );

        let cores = std::thread::available_parallelism()?.get();
        for &scale in &scales {
            let tiles = tile_grid(bounds, scale);
            println!("\nscale {scale}: {} tiles", tiles.len());
            let mut reference = None;
            let mut threads = vec![1, 2, 4, 8, cores];
            threads.sort_unstable();
            threads.dedup();
            for n in threads {
                reset_stats();
                let (elapsed, hashes) = render_parallel(Ctx(base), List(list), scale, &tiles, n)?;
                let stats = (0..3)
                    .map(|i| {
                        format!(
                            "{} {}/{}",
                            LOCK_NAMES[i],
                            LOCKS.contended[i].load(Relaxed),
                            LOCKS.acquired[i].load(Relaxed)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "  {n:>2} threads {:>10} | {:>7.1} tiles/s | contended/acquired: {stats}",
                    ms(elapsed),
                    tiles.len() as f64 / elapsed.as_secs_f64()
                );
                match &reference {
                    None => reference = Some(hashes),
                    Some(r) if *r != hashes => println!("  !! output differs from 1 thread"),
                    _ => {}
                }
            }
        }

        fz_drop_display_list(base, list);
        fz_drop_page(base, page);
        fz_drop_document(base, doc);
        fz_drop_context(base);
    }
    Ok(())
}

fn reset_stats() {
    for i in 0..3 {
        LOCKS.acquired[i].store(0, Relaxed);
        LOCKS.contended[i].store(0, Relaxed);
    }
}

fn tile_grid(b: fz_rect, scale: f32) -> Vec<fz_irect> {
    let w = ((b.x1 - b.x0) * scale).ceil() as i32;
    let h = ((b.y1 - b.y0) * scale).ceil() as i32;
    let mut out = Vec::new();
    for y in (0..h).step_by(TILE as usize) {
        for x in (0..w).step_by(TILE as usize) {
            out.push(fz_irect {
                x0: x,
                y0: y,
                x1: (x + TILE).min(w),
                y1: (y + TILE).min(h),
            });
        }
    }
    out
}

fn render_parallel(
    base: Ctx,
    list: List,
    scale: f32,
    tiles: &[fz_irect],
    threads: usize,
) -> Result<(Duration, Vec<u64>), String> {
    let next = AtomicUsize::new(0);
    let start = Instant::now();
    let results: Vec<Result<Vec<(usize, u64)>, String>> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    // Rebind so the closure captures the Send wrappers, not
                    // their raw-pointer fields (edition 2024 disjoint capture).
                    #[allow(clippy::redundant_locals)]
                    let base = base;
                    #[allow(clippy::redundant_locals)]
                    let list = list;
                    // SAFETY: cloning shares allocator, locks, store and glyph cache.
                    let ctx = unsafe { fz_clone_context(base.0) };
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Relaxed);
                        let Some(r) = tiles.get(i) else { break };
                        // SAFETY: ctx is this thread's context; list is finished.
                        match unsafe { render_tile(ctx, list.0, scale, *r) } {
                            Ok(h) => out.push((i, h)),
                            Err(e) => {
                                unsafe { fz_drop_context(ctx) };
                                return Err(e);
                            }
                        }
                    }
                    unsafe { fz_drop_context(ctx) };
                    Ok(out)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap_or_else(|_| Err("panic".into())))
            .collect()
    });
    let elapsed = start.elapsed();
    let mut hashes = vec![0; tiles.len()];
    for r in results {
        for (i, h) in r? {
            hashes[i] = h;
        }
    }
    Ok((elapsed, hashes))
}

unsafe fn render_tile(
    ctx: *mut fz_context,
    list: *mut fz_display_list,
    scale: f32,
    r: fz_irect,
) -> Result<u64, String> {
    unsafe {
        let mut err = ptr::null_mut();
        let pix = mupdf_new_pixmap(
            ctx,
            fz_device_rgb(ctx),
            r.x0,
            r.y0,
            r.x1 - r.x0,
            r.y1 - r.y0,
            true,
            &mut err,
        );
        check(err)?;
        fz_clear_pixmap_with_value(ctx, pix, 255);
        let dev = mupdf_new_draw_device(ctx, pix, r, &mut err);
        if let Err(e) = check(err) {
            fz_drop_pixmap(ctx, pix);
            return Err(e);
        }
        let ctm = fz_matrix {
            a: scale,
            b: 0.0,
            c: 0.0,
            d: scale,
            e: 0.0,
            f: 0.0,
        };
        let area = fz_rect {
            x0: r.x0 as f32,
            y0: r.y0 as f32,
            x1: r.x1 as f32,
            y1: r.y1 as f32,
        };
        mupdf_display_list_run(ctx, list, dev, ctm, area, ptr::null_mut(), &mut err);
        fz_close_device(ctx, dev);
        fz_drop_device(ctx, dev);
        let res = check(err);
        let len = (fz_pixmap_stride(ctx, pix) * (r.y1 - r.y0)) as usize;
        let samples = std::slice::from_raw_parts(fz_pixmap_samples(ctx, pix), len);
        let mut h = DefaultHasher::new();
        samples.hash(&mut h);
        fz_drop_pixmap(ctx, pix);
        res.map(|()| h.finish())
    }
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}
