// SPDX-License-Identifier: AGPL-3.0-or-later

// Developer tool: failing loudly on bad input is fine here.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Renders every page of a document at a few zoom levels, in 512 px tiles,
//! to smoke-test MuPDF on real files. Prints progress so a hard exit shows
//! where it happened.
//! Usage: cargo run -p fitz --example render_all -- <file.pdf> [scale...]

use std::io::Write;
use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else { return };
    let scales: Vec<f32> = args.filter_map(|s| s.parse().ok()).collect();
    let scales = if scales.is_empty() {
        vec![1.0, 2.0]
    } else {
        scales
    };
    let doc = fitz::Document::open(Path::new(&path)).expect("open");
    let n = doc.page_count().expect("count");
    for i in 0..n {
        print!("page {i}: ");
        std::io::stdout().flush().ok();
        let list = match doc.load_page(i).and_then(|p| p.to_display_list(true)) {
            Ok(l) => l,
            Err(e) => {
                println!("display list error: {e}");
                continue;
            }
        };
        let b = list.bounds();
        for &s in &scales {
            let (w, h) = (
                (b.width() * s).ceil() as i32,
                (b.height() * s).ceil() as i32,
            );
            for y in (0..h).step_by(512) {
                for x in (0..w).step_by(512) {
                    let tile = fitz::IRect::new(x, y, (x + 512).min(w), (y + 512).min(h));
                    if let Err(e) = list.render(fitz::Matrix::scale(s, s), tile, None) {
                        print!("[tile error s={s} {x},{y}: {e}] ");
                    }
                }
            }
            print!("x{s} ok ");
            std::io::stdout().flush().ok();
        }
        println!();
    }
    println!("done");
}
