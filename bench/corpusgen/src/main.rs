// SPDX-License-Identifier: AGPL-3.0-or-later

//! Generates the synthetic benchmark corpus into `bench/corpus/generated/`.
//!
//! Every file is deterministic (fixed seeds, no timestamps), so benchmark
//! numbers can be reproduced anywhere with `cargo run -p corpusgen --release`.

mod pdf;

use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pdf::{ObjId, PdfWriter, Rng, pdf_string};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const A0: (f32, f32) = (2384.0, 3370.0);
const LETTER: (f32, f32) = (612.0, 792.0);

fn main() -> Result<()> {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/generated");
    std::fs::create_dir_all(&out)?;

    // `corpusgen layers`: one plan per layer, for profiling (not part of the corpus).
    if std::env::args().nth(1).as_deref() == Some("layers") {
        for (i, name) in LAYERS.iter().enumerate() {
            let t = Instant::now();
            let bytes = plan_with(A0, 1_000_000, 0xA0, 1 << i);
            write(
                &out.join(format!("plan-a0-only-{}.pdf", name.to_lowercase())),
                &bytes,
                t,
            )?;
        }
        return Ok(());
    }

    type Job = Box<dyn Fn() -> Vec<u8>>;
    let jobs: Vec<(&str, Job)> = vec![
        ("plan-a0-layers.pdf", Box::new(|| plan(A0, 1_000_000, 0xA0))),
        ("paper-60.pdf", Box::new(|| paper(60, false, 0x60))),
        ("big-1000p.pdf", Box::new(|| paper(1000, true, 0x1000))),
    ];
    for (name, job) in jobs {
        let t = Instant::now();
        let bytes = job();
        write(&out.join(name), &bytes, t)?;
    }

    // Password-protected copy ("secret"), written by MuPDF itself.
    let t = Instant::now();
    let encrypted = out.join("encrypted-secret.pdf");
    encrypt(&out.join("paper-60.pdf"), &encrypted, "secret")?;
    report(&encrypted, t)?;

    let malformed = out.join("malformed");
    std::fs::create_dir_all(&malformed)?;
    let good = paper(3, false, 0xBAD);
    for (name, bytes) in malformed_files(&good) {
        let t = Instant::now();
        write(&malformed.join(name), &bytes, t)?;
    }

    // Small committed fixtures for unit tests (bench/corpus/fixtures/).
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/fixtures");
    std::fs::create_dir_all(fixtures.join("malformed"))?;
    let t = Instant::now();
    write(&fixtures.join("paper-3.pdf"), &good, t)?;
    let t = Instant::now();
    write(&fixtures.join("mixed-sizes-100.pdf"), &mixed_sizes(100), t)?;
    let t = Instant::now();
    write(
        &fixtures.join("clips-groups.pdf"),
        &clips_and_groups(0xC11),
        t,
    )?;
    let t = Instant::now();
    write(
        &fixtures.join("plan-a4.pdf"),
        &plan((595.0, 842.0), 20_000, 0xA4),
        t,
    )?;
    let t = Instant::now();
    encrypt(
        &fixtures.join("paper-3.pdf"),
        &fixtures.join("encrypted-secret.pdf"),
        "secret",
    )?;
    report(&fixtures.join("encrypted-secret.pdf"), t)?;
    for (name, bytes) in malformed_files(&good) {
        let t = Instant::now();
        write(&fixtures.join("malformed").join(name), &bytes, t)?;
    }
    Ok(())
}

fn write(path: &PathBuf, bytes: &[u8], started: Instant) -> Result<()> {
    std::fs::write(path, bytes)?;
    report(path, started)
}

fn report(path: &Path, started: Instant) -> Result<()> {
    let size = std::fs::metadata(path)?.len();
    println!(
        "{:<48} {:>10.2} MB  {:>6} ms",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default(),
        size as f64 / 1e6,
        started.elapsed().as_millis()
    );
    Ok(())
}

fn encrypt(src: &Path, dst: &Path, password: &str) -> Result<()> {
    use mupdf::pdf::{PdfDocument, PdfWriteOptions, document::Encryption};
    let doc = PdfDocument::open(src.to_string_lossy().as_ref())?;
    let mut opts = PdfWriteOptions::default();
    opts.set_encryption(Encryption::Aes256)
        .set_user_password(password)
        .set_owner_password(password);
    doc.save_with_options(dst.to_string_lossy().as_ref(), opts)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Architecture plan: one huge page, millions of path segments, OCG layers.
// ---------------------------------------------------------------------------

const LAYERS: [&str; 5] = ["Grid", "Walls", "Furniture", "Dimensions", "Labels"];

/// Builds a single-page vector plan with about `segments` path segments.
fn plan(size: (f32, f32), segments: usize, seed: u64) -> Vec<u8> {
    plan_with(size, segments, seed, u32::MAX)
}

/// Like [`plan`], emitting only the layers whose bit is set in `mask`.
fn plan_with(size: (f32, f32), segments: usize, seed: u64, mask: u32) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut w = PdfWriter::new();
    let catalog = w.alloc();
    let pages = w.alloc();
    let page = w.alloc();
    let content = w.alloc();
    let font = w.alloc();
    let ocgs: Vec<ObjId> = LAYERS.iter().map(|_| w.alloc()).collect();

    let (pw, ph) = size;
    let mut c = String::with_capacity(segments * 36);
    let layer = |c: &mut String, i: usize, body: &mut dyn FnMut(&mut String)| {
        // Generate masked-out layers too, so the RNG stream (and therefore
        // every other layer) stays identical to the full plan.
        let mut tmp = String::new();
        body(&mut tmp);
        if mask & (1 << i) != 0 {
            let _ = writeln!(c, "/OC /L{i} BDC");
            c.push_str(&tmp);
            c.push_str("EMC\n");
        }
    };

    // Grid: fine and coarse lines.
    layer(&mut c, 0, &mut |c| {
        c.push_str("0.85 0.88 0.92 RG 0.2 w\n");
        let mut x = 0.0;
        while x <= pw {
            let _ = writeln!(c, "{x:.1} 0 m {x:.1} {ph:.1} l");
            x += 10.0;
        }
        let mut y = 0.0;
        while y <= ph {
            let _ = writeln!(c, "0 {y:.1} m {pw:.1} {y:.1} l");
            y += 10.0;
        }
        c.push_str("S\n");
    });

    // Walls: rooms stroked edge by edge, like typical CAD exports (~20 %).
    let walls = segments / 5 / 4;
    layer(&mut c, 1, &mut |c| {
        c.push_str("0.1 0.1 0.12 RG 1.6 w 2 J\n");
        for _ in 0..walls {
            let (x, y) = (rng.range(20.0, pw - 220.0), rng.range(20.0, ph - 220.0));
            let (rw, rh) = (rng.range(8.0, 200.0), rng.range(8.0, 200.0));
            let _ = write!(
                c,
                "{x:.2} {y:.2} m {:.2} {y:.2} l S\n{:.2} {y:.2} m {:.2} {:.2} l S\n\
                 {:.2} {:.2} m {x:.2} {:.2} l S\n{x:.2} {:.2} m {x:.2} {y:.2} l S\n",
                x + rw,
                x + rw,
                x + rw,
                y + rh,
                x + rw,
                y + rh,
                y + rh,
                y + rh
            );
        }
    });

    // Furniture: small closed Bézier shapes and polylines (~60 %).
    let furniture = segments * 3 / 5 / 6;
    layer(&mut c, 2, &mut |c| {
        c.push_str("0.25 0.35 0.6 RG 0.5 w\n");
        for i in 0..furniture {
            let (x, y) = (rng.range(10.0, pw - 30.0), rng.range(10.0, ph - 30.0));
            let r = rng.range(1.0, 12.0);
            if i % 2 == 0 {
                let k = r * 0.552;
                let _ = writeln!(
                    c,
                    "{:.2} {y:.2} m {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} c \
                     {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} c \
                     {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} c \
                     {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} c S",
                    x + r,
                    x + r,
                    y + k,
                    x + k,
                    y + r,
                    y + r,
                    x - k,
                    y + r,
                    x - r,
                    y + k,
                    x - r,
                    x - r,
                    y - k,
                    x - k,
                    y - r,
                    y - r,
                    x + k,
                    y - r,
                    x + r,
                    y - k,
                    x + r
                );
                // A Bézier circle counts as 4 segments; pad with 2 lines.
                let _ = writeln!(
                    c,
                    "{x:.2} {y:.2} m {:.2} {:.2} l {:.2} {:.2} l S",
                    x + r,
                    y,
                    x,
                    y + r
                );
            } else {
                let _ = write!(c, "{x:.2} {y:.2} m");
                let (mut px, mut py) = (x, y);
                for _ in 0..6 {
                    px += rng.range(-r, r);
                    py += rng.range(-r, r);
                    let _ = write!(c, " {px:.2} {py:.2} l");
                }
                c.push_str(" S\n");
            }
        }
    });

    // Dimensions: extension lines, filled arrowheads and numbers (~20 %).
    let dims = segments / 5 / 5;
    layer(&mut c, 3, &mut |c| {
        c.push_str("0.7 0.15 0.15 RG 0.7 0.15 0.15 rg 0.3 w\n");
        for _ in 0..dims {
            let (x, y) = (rng.range(20.0, pw - 120.0), rng.range(20.0, ph - 20.0));
            let len = rng.range(10.0, 100.0);
            let _ = writeln!(
                c,
                "{x:.2} {y:.2} m {:.2} {y:.2} l S {x:.2} {:.2} m {x:.2} {:.2} l S \
                 {:.2} {:.2} m {:.2} {:.2} l S {x:.2} {y:.2} m {:.2} {:.2} l {:.2} {:.2} l f",
                x + len,
                y - 3.0,
                y + 3.0,
                x + len,
                y - 3.0,
                x + len,
                y + 3.0,
                x + 3.0,
                y + 1.0,
                x + 3.0,
                y - 1.0
            );
        }
    });

    // Labels: room names and dimension values in Helvetica.
    layer(&mut c, 4, &mut |c| {
        c.push_str("0 0 0 rg BT\n");
        for i in 0..(segments / 200) {
            let (x, y) = (rng.range(20.0, pw - 80.0), rng.range(20.0, ph - 20.0));
            let size = rng.range(3.0, 9.0);
            let label = pdf_string(&format!("R{i:05} {:.2} m2", rng.range(4.0, 60.0)));
            let _ = writeln!(c, "/F1 {size:.1} Tf 1 0 0 1 {x:.2} {y:.2} Tm {label} Tj");
        }
        c.push_str("ET\n");
    });

    let props: String = ocgs
        .iter()
        .enumerate()
        .map(|(i, id)| format!("/L{i} {id} 0 R "))
        .collect();
    let refs: String = ocgs.iter().map(|id| format!("{id} 0 R ")).collect();
    for (id, name) in ocgs.iter().zip(LAYERS) {
        w.obj(*id, &format!("<< /Type /OCG /Name {} >>", pdf_string(name)));
    }
    w.obj(
        font,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    w.stream(content, "", c.as_bytes(), true);
    w.obj(
        page,
        &format!(
            "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {pw} {ph}] /Contents {content} 0 R \
             /Resources << /Font << /F1 {font} 0 R >> /Properties << {props}>> >> >>"
        ),
    );
    w.obj(
        pages,
        &format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    w.obj(
        catalog,
        &format!(
            "<< /Type /Catalog /Pages {pages} 0 R /OCProperties << /OCGs [{refs}] \
             /D << /Name (Default) /Order [{refs}] /ON [{refs}] /OFF [] >> >> >>"
        ),
    );
    w.finish(catalog, None)
}

// ---------------------------------------------------------------------------
// Paper-like documents: text, vector figures and optionally large images.
// ---------------------------------------------------------------------------

const SYLLABLES: [&str; 24] = [
    "ra", "ten", "lo", "mi", "sor", "que", "dan", "vel", "ip", "sum", "cor", "tis", "al", "ne",
    "gra", "du", "fi", "po", "mar", "es", "ul", "ta", "ber", "on",
];

fn words(rng: &mut Rng, n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        if i > 0 {
            s.push(' ');
        }
        for _ in 0..=rng.below(3) {
            s.push_str(SYLLABLES[rng.below(SYLLABLES.len() as u32) as usize]);
        }
    }
    s
}

/// Builds a letter-size document. With `images`, each page embeds a distinct
/// incompressible 200x200 RGB image, which makes 1000 pages exceed 100 MB.
fn paper(page_count: usize, images: bool, seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut w = PdfWriter::new();
    let catalog = w.alloc();
    let pages = w.alloc();
    let info = w.alloc();
    let font = w.alloc();
    let bold = w.alloc();
    let (pw, ph) = LETTER;
    let mut kids = String::new();

    for p in 0..page_count {
        let page = w.alloc();
        let content = w.alloc();
        let image = images.then(|| w.alloc());
        let _ = write!(kids, "{page} 0 R ");

        let mut c = String::with_capacity(8 << 10);
        let mut y = 720.0;
        c.push_str("BT\n");
        if p == 0 {
            let _ = writeln!(
                c,
                "/F2 18 Tf 72 {y} Td {} Tj",
                pdf_string("A Synthetic Paper for Rendering Benchmarks")
            );
            y -= 40.0;
        }
        let _ = writeln!(c, "/F1 10 Tf 12 TL 1 0 0 1 72 {y} Tm");
        let figure = p % 3 == 1;
        let lines = if figure { 26 } else { 52 } - if p == 0 { 3 } else { 0 };
        for _ in 0..lines {
            let _ = writeln!(c, "{} Tj T*", pdf_string(&words(&mut rng, 11)));
        }
        c.push_str("ET\n");
        if figure {
            // Axes plus a 400-point polyline, like a plotted measurement.
            let (x0, y0, fw, fh) = (90.0, 90.0, 430.0, 250.0);
            let _ = writeln!(
                c,
                "0 0 0 RG 0.8 w {x0} {y0} m {} {y0} l S {x0} {y0} m {x0} {} l S",
                x0 + fw,
                y0 + fh
            );
            c.push_str("0.2 0.4 0.8 RG 0.6 w\n");
            let mut v = fh / 2.0;
            for i in 0..400 {
                v = (v + rng.range(-6.0, 6.0)).clamp(0.0, fh);
                let x = x0 + fw * i as f32 / 399.0;
                let _ = write!(
                    c,
                    "{x:.2} {:.2} {} ",
                    y0 + v,
                    if i == 0 { "m" } else { "l" }
                );
            }
            c.push_str("S\n");
        }
        if let Some(img) = image {
            let _ = writeln!(c, "q 120 0 0 120 {} 36 cm /Im0 Do Q", pw - 72.0 - 120.0);
            let mut px = vec![0u8; 200 * 200 * 3];
            for chunk in px.chunks_mut(8) {
                let r = rng.next_u64().to_le_bytes();
                chunk.copy_from_slice(&r[..chunk.len()]);
            }
            w.stream(
                img,
                "/Type /XObject /Subtype /Image /Width 200 /Height 200 /ColorSpace /DeviceRGB /BitsPerComponent 8",
                &px,
                false,
            );
        }
        let _ = writeln!(c, "BT /F1 9 Tf 300 30 Td ({}) Tj ET", p + 1);
        w.stream(content, "", c.as_bytes(), true);
        let xobj = image
            .map(|i| format!(" /XObject << /Im0 {i} 0 R >>"))
            .unwrap_or_default();
        w.obj(
            page,
            &format!(
                "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {pw} {ph}] /Contents {content} 0 R \
                 /Resources << /Font << /F1 {font} 0 R /F2 {bold} 0 R >>{xobj} >> >>"
            ),
        );
    }

    w.obj(
        font,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    w.obj(
        bold,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>",
    );
    w.obj(
        info,
        "<< /Title (Synthetic benchmark paper) /Producer (corpusgen) >>",
    );
    w.obj(
        pages,
        &format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>"),
    );
    w.obj(catalog, &format!("<< /Type /Catalog /Pages {pages} 0 R >>"));
    w.finish(catalog, Some(info))
}

/// Pages alternating in blocks of ten between Letter portrait and A4
/// landscape, more than `doc::EAGER_PAGES`, so later sizes start as wrong
/// estimates and must be corrected in the background.
fn mixed_sizes(count: usize) -> Vec<u8> {
    let mut w = PdfWriter::new();
    let catalog = w.alloc();
    let pages = w.alloc();
    let font = w.alloc();
    let mut kids = String::new();
    for i in 0..count {
        let page = w.alloc();
        let content = w.alloc();
        let _ = write!(kids, "{page} 0 R ");
        let (pw, ph) = if (i / 10) % 2 == 0 {
            (612.0, 792.0)
        } else {
            (842.0, 595.0)
        };
        let body = format!("BT /F1 48 Tf 72 72 Td ({}) Tj ET", i + 1);
        w.stream(content, "", body.as_bytes(), false);
        w.obj(
            page,
            &format!(
                "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {pw} {ph}] /Contents {content} 0 R                  /Resources << /Font << /F1 {font} 0 R >> >> >>"
            ),
        );
    }
    w.obj(
        font,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    w.obj(
        pages,
        &format!("<< /Type /Pages /Kids [{kids}] /Count {count} >>"),
    );
    w.obj(catalog, &format!("<< /Type /Catalog /Pages {pages} 0 R >>"));
    w.finish(catalog, None)
}

/// A page full of nested clips and transparency groups: a render aborted
/// half-way leaves them open on the draw device's stack (regression fixture
/// for the uncaught `fz_close_device` exception).
fn clips_and_groups(seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut w = PdfWriter::new();
    let catalog = w.alloc();
    let pages = w.alloc();
    let page = w.alloc();
    let content = w.alloc();
    let group = w.alloc();
    let gs = w.alloc();
    let (pw, ph) = (595.0f32, 842.0f32);

    let mut g = String::new();
    for _ in 0..3000 {
        let (x, y) = (rng.range(0.0, 200.0), rng.range(0.0, 200.0));
        let _ = writeln!(
            g,
            "{:.2} {:.2} {:.2} rg {x:.2} {y:.2} m {:.2} {:.2} l {:.2} {y:.2} l f",
            rng.range(0.0, 1.0),
            rng.range(0.0, 1.0),
            rng.range(0.0, 1.0),
            x + 20.0,
            y + 30.0,
            x + 40.0
        );
    }
    w.stream(group, "/Type /XObject /Subtype /Form /BBox [0 0 240 240] /Group << /S /Transparency /K true >> /Resources << >>", g.as_bytes(), true);
    w.obj(gs, "<< /Type /ExtGState /ca 0.6 /BM /Multiply >>");

    let mut c = String::new();
    for i in 0..60 {
        let (x, y) = ((i % 6) as f32 * 95.0 + 10.0, (i / 6) as f32 * 82.0 + 10.0);
        let _ = writeln!(
            c,
            "q {x:.1} {y:.1} 90 78 re W n q /G0 gs 0.35 0 0 0.3 {x:.1} {y:.1} cm /X0 Do Q Q"
        );
    }
    w.stream(content, "", c.as_bytes(), true);
    w.obj(page, &format!(
        "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {pw} {ph}] /Contents {content} 0 R          /Group << /S /Transparency /CS /DeviceRGB >> /Resources << /XObject << /X0 {group} 0 R >> /ExtGState << /G0 {gs} 0 R >> >> >>"
    ));
    w.obj(
        pages,
        &format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>"),
    );
    w.obj(catalog, &format!("<< /Type /Catalog /Pages {pages} 0 R >>"));
    w.finish(catalog, None)
}

// ---------------------------------------------------------------------------
// Malformed files: the reader must open them (after repair) or fail cleanly.
// ---------------------------------------------------------------------------

const ONE_PAGE: &str = "<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>";

fn malformed_files(good: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    let mut rng = Rng::new(0xDEAD);
    let garbage: Vec<u8> = (0..4096).map(|_| rng.next_u64() as u8).collect();
    let truncated = good[..good.len() / 2].to_vec();

    // Point startxref at a bogus offset: MuPDF must rebuild the xref.
    let mut bad_xref = good.to_vec();
    if let Some(pos) = find(&bad_xref, b"startxref\n") {
        let tail = b"startxref\n12345\n%%EOF\n";
        bad_xref.truncate(pos);
        bad_xref.extend_from_slice(tail);
    }

    let mut no_trailer = good.to_vec();
    if let Some(pos) = find(&no_trailer, b"xref\n0 ") {
        no_trailer.truncate(pos);
    }

    // Wrong stream /Length values.
    let bad_length = replace_all(good, b"/Length ", b"/Length 9");

    let tiny = |body: &str| -> Vec<u8> {
        let mut w = PdfWriter::new();
        let catalog = w.alloc();
        let pages = w.alloc();
        let page = w.alloc();
        w.obj(catalog, &format!("<< /Type /Catalog /Pages {pages} 0 R >>"));
        w.obj(
            pages,
            &body
                .replace("{page}", &page.to_string())
                .replace("{pages}", &pages.to_string()),
        );
        w.obj(
            page,
            &format!("<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 612 792] >>"),
        );
        w.finish(catalog, None)
    };

    let mut deep = String::from("<< /Type /Pages /Kids [{page} 0 R] /Count 1 /Deep ");
    deep.push_str(&"[".repeat(100_000));
    deep.push_str(&"]".repeat(100_000));
    deep.push_str(" >>");

    vec![
        ("empty.pdf", Vec::new()),
        ("garbage.pdf", garbage),
        ("truncated.pdf", truncated),
        ("bad-startxref.pdf", bad_xref),
        ("no-xref-no-trailer.pdf", no_trailer),
        ("bad-stream-length.pdf", bad_length),
        (
            "huge-page-count.pdf",
            tiny("<< /Type /Pages /Kids [{page} 0 R] /Count 2000000000 >>"),
        ),
        (
            "cyclic-page-tree.pdf",
            tiny("<< /Type /Pages /Kids [{pages} 0 R {page} 0 R] /Count 2 >>"),
        ),
        (
            "zero-mediabox.pdf",
            // Same byte length, so the xref offsets stay valid.
            replace_all(
                &tiny(ONE_PAGE),
                b"/MediaBox [0 0 612 792]",
                b"/MediaBox [0 0 0 0    ]",
            ),
        ),
        ("deep-nesting.pdf", tiny(&deep)),
    ]
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

fn replace_all(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(hay.len());
    let mut i = 0;
    while i < hay.len() {
        if hay[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(hay[i]);
            i += 1;
        }
    }
    out
}
