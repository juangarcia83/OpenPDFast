// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;

use fitz::{Cookie, Document, ErrorKind, IRect, Matrix, MetadataKey, Rect};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/corpus/fixtures")
        .join(name)
}

#[test]
fn opens_and_reads_pages() {
    let doc = Document::open(&fixture("paper-3.pdf")).unwrap();
    assert!(!doc.needs_password().unwrap());
    assert_eq!(doc.page_count().unwrap(), 3);
    assert_eq!(
        doc.metadata(MetadataKey::Title).unwrap().as_deref(),
        Some("Synthetic benchmark paper")
    );
    assert_eq!(doc.metadata(MetadataKey::Author).unwrap(), None);
    let bounds = doc.load_page(0).unwrap().bounds().unwrap();
    assert_eq!(bounds, Rect::new(0.0, 0.0, 612.0, 792.0));
    assert!(doc.load_page(3).is_err());
}

#[test]
fn renders_a_tile_with_content() {
    let doc = Document::open(&fixture("paper-3.pdf")).unwrap();
    let list = doc.load_page(0).unwrap().to_display_list(true).unwrap();
    let tile = IRect::new(0, 0, 256, 128);
    let pix = list.render(Matrix::scale(1.0, 1.0), tile, None).unwrap();
    assert_eq!((pix.width(), pix.height()), (256, 128));
    let rgba = pix.to_rgba().unwrap();
    assert_eq!(rgba.len(), 256 * 128 * 4);
    assert!(
        rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255),
        "opaque"
    );
    assert!(
        rgba.as_chunks::<4>().0.iter().any(|p| p[0] < 128),
        "title text is drawn"
    );
}

#[test]
fn tiles_render_identically_from_many_threads() {
    let doc = Document::open(&fixture("plan-a4.pdf")).unwrap();
    let list = Arc::new(doc.load_page(0).unwrap().to_display_list(true).unwrap());
    let tile = IRect::new(256, 256, 512, 512);
    let reference = list
        .render(Matrix::scale(2.0, 2.0), tile, None)
        .unwrap()
        .to_rgba()
        .unwrap();
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let list = Arc::clone(&list);
            std::thread::spawn(move || {
                list.render(Matrix::scale(2.0, 2.0), tile, None)
                    .unwrap()
                    .to_rgba()
                    .unwrap()
            })
        })
        .collect();
    for h in handles {
        assert!(h.join().unwrap() == reference);
    }
}

#[test]
fn extracted_region_renders_like_the_full_list() {
    let doc = Document::open(&fixture("plan-a4.pdf")).unwrap();
    let list = doc.load_page(0).unwrap().to_display_list(true).unwrap();
    let cell = list.extract(Rect::new(0.0, 0.0, 256.0, 256.0)).unwrap();
    let tile = IRect::new(0, 0, 512, 512);
    let full = list
        .render(Matrix::scale(2.0, 2.0), tile, None)
        .unwrap()
        .to_rgba()
        .unwrap();
    let part = cell
        .render(Matrix::scale(2.0, 2.0), tile, None)
        .unwrap()
        .to_rgba()
        .unwrap();
    assert!(full == part);
}

#[test]
fn aborted_cookie_stops_rendering() {
    let doc = Document::open(&fixture("plan-a4.pdf")).unwrap();
    let list = doc.load_page(0).unwrap().to_display_list(true).unwrap();
    let cookie = Cookie::new();
    cookie.abort();
    assert!(cookie.is_aborted());
    // MuPDF stops early and either reports an abort or returns a partial tile.
    match list.render(
        Matrix::scale(2.0, 2.0),
        IRect::new(0, 0, 512, 512),
        Some(&cookie),
    ) {
        Ok(_) => {}
        Err(e) => assert_eq!(e.kind(), ErrorKind::Aborted),
    }
}

#[test]
fn password_protected_documents() {
    let mut doc = Document::open(&fixture("encrypted-secret.pdf")).unwrap();
    assert!(doc.needs_password().unwrap());
    assert!(!doc.authenticate("wrong").unwrap());
    assert!(doc.authenticate("secret").unwrap());
    assert_eq!(doc.page_count().unwrap(), 3);
}

#[test]
fn malformed_files_never_panic() {
    let dir = fixture("malformed");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let Ok(doc) = Document::open(&path) else {
            continue;
        };
        let pages = doc.page_count().unwrap_or(0).min(3);
        for i in 0..pages {
            if let Ok(page) = doc.load_page(i) {
                let _ = page.bounds();
                if let Ok(list) = page.to_display_list(true) {
                    let _ = list.render(Matrix::IDENTITY, IRect::new(0, 0, 64, 64), None);
                }
            }
        }
    }
}

#[test]
fn fast_page_sizes_match_loaded_page_bounds() {
    let mut files = vec![
        fixture("paper-3.pdf"),
        fixture("plan-a4.pdf"),
        fixture("encrypted-secret.pdf"),
    ];
    files.extend(
        std::fs::read_dir(fixture("malformed"))
            .unwrap()
            .map(|e| e.unwrap().path()),
    );
    for path in files {
        let Ok(mut doc) = Document::open(&path) else {
            continue;
        };
        let _ = doc.authenticate("secret");
        for i in 0..doc.page_count().unwrap_or(0).min(10) {
            let Ok(bounds) = doc.load_page(i).and_then(|p| p.bounds()) else {
                continue;
            };
            let (w, h) = doc.page_size_fast(i).unwrap().unwrap();
            assert!(
                (w - bounds.width()).abs() < 0.01,
                "{}: page {i}",
                path.display()
            );
            assert!(
                (h - bounds.height()).abs() < 0.01,
                "{}: page {i}",
                path.display()
            );
        }
    }
}

/// Regression: aborting a render half-way left groups and clips on the draw
/// device's stack; closing that device threw an exception outside any
/// `fz_try`, and MuPDF exited the whole process.
#[test]
fn aborting_mid_render_never_kills_the_process() {
    let doc = Document::open(&fixture("clips-groups.pdf")).unwrap();
    let list = Arc::new(doc.load_page(0).unwrap().to_display_list(true).unwrap());
    let tile = IRect::new(0, 0, 512, 512);
    let mut aborted = 0;
    for i in 0..200u64 {
        let cookie = Arc::new(Cookie::new());
        let killer = {
            let cookie = Arc::clone(&cookie);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_micros(50 + (i * 37) % 2000));
                cookie.abort();
            })
        };
        // MuPDF stops early without reporting an error: the result is a
        // partial tile, and the device is left with open groups and clips.
        let _ = list.render(Matrix::scale(2.0, 2.0), tile, Some(&cookie));
        if cookie.is_aborted() {
            aborted += 1;
        }
        killer.join().unwrap();
    }
    // Reaching this line is the assertion; also check the test aborted some runs.
    assert!(
        aborted > 0,
        "no render was aborted; make the fixture heavier"
    );
}
