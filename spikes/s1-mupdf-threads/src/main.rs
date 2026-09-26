// SPDX-License-Identifier: AGPL-3.0-or-later

//! Spike S1: can one MuPDF display list be rasterized into tiles from many
//! threads at once, safely and with a real speed-up?
//!
//! Usage: s1-mupdf-threads <file.pdf> [page] [scale...]

use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use mupdf::{Colorspace, Device, DisplayList, Document, IRect, Matrix, Pixmap, Rect};

const TILE: i32 = 512;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .ok_or("usage: s1-mupdf-threads <file.pdf> [page] [scale...]")?;
    let page_no: i32 = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(0);
    let scales: Vec<f32> = if args.len() > 3 {
        args[3..]
            .iter()
            .map(|s| s.parse())
            .collect::<Result<_, _>>()?
    } else {
        vec![1.0, 2.0, 4.0]
    };
    let cores = std::thread::available_parallelism()?.get();

    let t = Instant::now();
    let doc = Document::open(path.as_str())?;
    let open = t.elapsed();
    let t = Instant::now();
    let pages = doc.page_count()?;
    let page = doc.load_page(page_no)?;
    let bounds = page.bounds()?;
    let load = t.elapsed();
    let t = Instant::now();
    let list = page.to_display_list(true)?;
    let build = t.elapsed();
    println!("file: {path}");
    println!(
        "open {} | {pages} pages | load page {page_no} {} | bounds {:.0}x{:.0} pt | display list {}",
        ms(open),
        ms(load),
        bounds.width(),
        bounds.height(),
        ms(build)
    );

    for &scale in &scales {
        let tiles = tile_grid(&bounds, scale);
        println!(
            "\nscale {scale}: page {:.0}x{:.0} px, {} tiles of {TILE}px",
            bounds.width() * scale,
            bounds.height() * scale,
            tiles.len()
        );

        // Baseline: re-interpret the page content for every tile (no display
        // list). Very slow on the plan, so opt-in with S1_BASELINE=1.
        let reference: Vec<u64> = if std::env::var_os("S1_BASELINE").is_some() {
            let t = Instant::now();
            let r = tiles
                .iter()
                .map(|r| render_page_tile(&page, scale, *r))
                .collect::<Result<_, _>>()?;
            report("page.run, 1 thread", tiles.len(), t.elapsed(), None);
            r
        } else {
            render_parallel(&list, scale, &tiles, 1)?.2
        };

        let mut threads = vec![1, 2, 4, 8, cores];
        threads.sort_unstable();
        threads.dedup();
        for n in threads {
            let (elapsed, first, hashes) = render_parallel(&list, scale, &tiles, n)?;
            report(
                &format!("display list, {n:>2} threads"),
                tiles.len(),
                elapsed,
                Some(first),
            );
            if hashes != reference {
                let diff = hashes
                    .iter()
                    .zip(&reference)
                    .filter(|(a, b)| a != b)
                    .count();
                println!("  !! {diff} tiles differ from the single-threaded page.run output");
            }
        }

        // Spatially partitioned display lists: one sub-list per cell of
        // CELL_PT page points, recorded from the master list with culling.
        // Tiles at scale >= 512/CELL_PT fall inside exactly one cell.
        if scale * CELL_PT >= TILE as f32 {
            let t = Instant::now();
            let cells = partition(&list, &bounds)?;
            println!(
                "  partitioned into {} cells of {CELL_PT}pt in {}",
                cells.len(),
                ms(t.elapsed())
            );
            for n in [1, 2, 4, cores] {
                let (elapsed, first, hashes) = render_cells(&cells, scale, &tiles, n)?;
                report(
                    &format!("cell lists, {n:>2} threads"),
                    tiles.len(),
                    elapsed,
                    Some(first),
                );
                if hashes != reference {
                    let diff = hashes
                        .iter()
                        .zip(&reference)
                        .filter(|(a, b)| a != b)
                        .count();
                    println!("  !! {diff} tiles differ from the page.run output");
                }
            }
        }
    }
    Ok(())
}

const CELL_PT: f32 = 512.0;

struct Cell {
    rect: Rect,
    list: DisplayList,
}

fn partition(list: &DisplayList, bounds: &Rect) -> Result<Vec<Cell>, mupdf::Error> {
    let mut cells = Vec::new();
    let mut y = bounds.y0;
    while y < bounds.y1 {
        let mut x = bounds.x0;
        while x < bounds.x1 {
            let rect = Rect::new(
                x,
                y,
                (x + CELL_PT).min(bounds.x1),
                (y + CELL_PT).min(bounds.y1),
            );
            let mut sub = DisplayList::new(rect)?;
            {
                let dev = Device::from_display_list(&mut sub)?;
                list.run(&dev, &Matrix::IDENTITY, rect)?;
            }
            cells.push(Cell { rect, list: sub });
            x += CELL_PT;
        }
        y += CELL_PT;
    }
    Ok(cells)
}

fn render_cells(
    cells: &[Cell],
    scale: f32,
    tiles: &[IRect],
    threads: usize,
) -> Result<(Duration, Duration, Vec<u64>), Box<dyn Error>> {
    let next = AtomicUsize::new(0);
    let start = Instant::now();
    let first = std::sync::Mutex::new(None::<Duration>);
    let results: Vec<Vec<(usize, u64)>> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(r) = tiles.get(i) else { break };
                        let (cx, cy) = (r.x0 as f32 / scale + 1.0, r.y0 as f32 / scale + 1.0);
                        let cell = cells.iter().find(|c| c.rect.contains(cx, cy));
                        let h = cell
                            .map(|c| render_list_tile(&c.list, scale, *r).unwrap_or(0))
                            .unwrap_or(0);
                        if let Ok(mut f) = first.lock() {
                            f.get_or_insert_with(|| start.elapsed());
                        }
                        out.push((i, h));
                    }
                    out
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap_or_default())
            .collect()
    });
    let elapsed = start.elapsed();
    let mut hashes = vec![0; tiles.len()];
    for (i, h) in results.into_iter().flatten() {
        hashes[i] = h;
    }
    let first = first.into_inner().ok().flatten().unwrap_or_default();
    Ok((elapsed, first, hashes))
}

fn tile_grid(bounds: &Rect, scale: f32) -> Vec<IRect> {
    let w = (bounds.width() * scale).ceil() as i32;
    let h = (bounds.height() * scale).ceil() as i32;
    let mut out = Vec::new();
    for y in (0..h).step_by(TILE as usize) {
        for x in (0..w).step_by(TILE as usize) {
            out.push(IRect::new(x, y, (x + TILE).min(w), (y + TILE).min(h)));
        }
    }
    out
}

fn new_tile(r: IRect) -> Result<(Pixmap, Matrix), mupdf::Error> {
    let mut pix = Pixmap::new_with_rect(&Colorspace::device_rgb(), r, true)?;
    pix.clear_with(255)?;
    Ok((pix, Matrix::IDENTITY))
}

fn digest(pix: &Pixmap) -> u64 {
    let mut h = DefaultHasher::new();
    pix.samples().hash(&mut h);
    h.finish()
}

fn render_page_tile(page: &mupdf::Page, scale: f32, r: IRect) -> Result<u64, mupdf::Error> {
    let (pix, _) = new_tile(r)?;
    {
        let dev = Device::from_pixmap_with_clip(&pix, r)?;
        page.run(&dev, &Matrix::new_scale(scale, scale))?;
    }
    Ok(digest(&pix))
}

fn render_list_tile(list: &DisplayList, scale: f32, r: IRect) -> Result<u64, mupdf::Error> {
    let (pix, _) = new_tile(r)?;
    {
        let dev = Device::from_pixmap(&pix)?;
        let area = Rect::new(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32);
        list.run(&dev, &Matrix::new_scale(scale, scale), area)?;
    }
    Ok(digest(&pix))
}

/// Work-stealing over an atomic index; returns (total, first tile, digests).
fn render_parallel(
    list: &DisplayList,
    scale: f32,
    tiles: &[IRect],
    threads: usize,
) -> Result<(Duration, Duration, Vec<u64>), Box<dyn Error>> {
    let next = AtomicUsize::new(0);
    let start = Instant::now();
    let first = std::sync::Mutex::new(None::<Duration>);
    let results: Vec<Vec<(usize, u64)>> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(r) = tiles.get(i) else { break };
                        let h = render_list_tile(list, scale, *r).unwrap_or(0);
                        if let Ok(mut f) = first.lock() {
                            f.get_or_insert_with(|| start.elapsed());
                        }
                        out.push((i, h));
                    }
                    out
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap_or_default())
            .collect()
    });
    let elapsed = start.elapsed();
    let mut hashes = vec![0; tiles.len()];
    for (i, h) in results.into_iter().flatten() {
        hashes[i] = h;
    }
    let first = first.into_inner().ok().flatten().unwrap_or_default();
    Ok((elapsed, first, hashes))
}

fn report(label: &str, n: usize, elapsed: Duration, first: Option<Duration>) {
    let first = first
        .map(|f| format!(" | first tile {}", ms(f)))
        .unwrap_or_default();
    println!(
        "  {label:<26} {:>9} total | {:>8.1} tiles/s | {:>6.2} ms/tile{first}",
        ms(elapsed),
        n as f64 / elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / n as f64
    );
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}
