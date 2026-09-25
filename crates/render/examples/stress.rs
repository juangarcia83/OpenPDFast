// SPDX-License-Identifier: AGPL-3.0-or-later

// Developer tool: failing loudly on bad input is fine here.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Drives the scheduler like the viewer does while scrolling and zooming
//! quickly (lots of cancellations), to smoke-test real documents.
//! Usage: cargo run -p render --example stress -- <file.pdf>

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use render::{
    MupdfRenderer, PageRect, PageRegion, Scheduler, SchedulerConfig, Viewport, ZoomLevel,
};

fn main() {
    tracing_subscriber_init();
    let path = std::env::args().nth(1).expect("usage: stress <file.pdf>");
    let doc = doc::Document::open(Path::new(&path), None).expect("open");
    let pages = doc.info().pages.clone();
    let tiles = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&tiles);
    let sink = Arc::new(move |_| {
        counter.fetch_add(1, Ordering::Relaxed);
    });
    let s = Scheduler::new(MupdfRenderer::new(doc), SchedulerConfig::default(), sink)
        .expect("scheduler");
    for step in 0..400u32 {
        let page = (step / 3) % pages.len() as u32;
        let size = pages[page as usize];
        let level = ZoomLevel((step % 7) as i8 - 1);
        let rect = PageRect {
            x0: 0.0,
            y0: 0.0,
            x1: size.width,
            y1: size.height,
        };
        s.set_viewport(Viewport {
            visible: vec![PageRegion { page, rect, level }],
            nearby: vec![PageRegion {
                page: (page + 1) % pages.len() as u32,
                rect,
                level,
            }],
            evicted: Vec::new(),
        });
        std::thread::sleep(Duration::from_millis(5));
        if step % 50 == 0 {
            println!(
                "step {step}: {} tiles, {:?}",
                tiles.load(Ordering::Relaxed),
                s.stats()
            );
        }
    }
    while !s.is_idle() {
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("done: {} tiles", tiles.load(Ordering::Relaxed));
}

fn tracing_subscriber_init() {
    tracing_subscriber::fmt()
        .with_env_filter("warn,mupdf=debug")
        .with_writer(std::io::stderr)
        .init();
}
