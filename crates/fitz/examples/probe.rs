// SPDX-License-Identifier: AGPL-3.0-or-later

//! Prints open time, page count and the cost of reading every page box.
//! Usage: cargo run -p fitz --release --example probe -- <file.pdf>...

use std::path::Path;
use std::time::Instant;

fn main() {
    for arg in std::env::args().skip(1) {
        let t = Instant::now();
        let doc = match fitz::Document::open(Path::new(&arg)) {
            Ok(d) => d,
            Err(e) => {
                println!("{arg}: open failed: {e}");
                continue;
            }
        };
        let open = t.elapsed();
        let count = doc.page_count();
        let t = Instant::now();
        let mut ok = 0;
        let mut first_err = None;
        for i in 0..count.as_ref().copied().unwrap_or(0).min(5000) {
            match doc.load_page(i).and_then(|p| p.bounds()) {
                Ok(_) => ok += 1,
                Err(e) => {
                    first_err.get_or_insert(e.to_string());
                }
            }
        }
        let t_fast = Instant::now();
        let mut mismatches = 0;
        let n = count.as_ref().copied().unwrap_or(0).min(5000);
        let fast: Vec<_> = (0..n).map(|i| doc.page_size_fast(i)).collect();
        let fast_ms = t_fast.elapsed().as_secs_f64() * 1e3;
        for (i, f) in fast.iter().enumerate() {
            let slow = doc.load_page(i).and_then(|p| p.bounds()).ok();
            if let (Ok(Some((w, h))), Some(b)) = (f, slow)
                && ((w - b.width()).abs() > 0.01 || (h - b.height()).abs() > 0.01)
            {
                mismatches += 1;
            }
        }
        println!("  fast page sizes: {n} in {fast_ms:.2} ms, {mismatches} mismatches");
        println!(
            "{arg}: open {:.2} ms | pages {count:?} | {ok} boxes in {:.2} ms | first error: {first_err:?}",
            open.as_secs_f64() * 1e3,
            t.elapsed().as_secs_f64() * 1e3
        );
    }
}
