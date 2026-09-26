// SPDX-License-Identifier: AGPL-3.0-or-later

//! Lists a document's layers and optionally toggles one by name.
//! Usage: cargo run -p fitz --example layers -- <file.pdf> [name]

// Developer tool: failing loudly on bad input is fine here.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: layers <file.pdf> [name]");
    let mut doc = fitz::Document::open(Path::new(&path)).expect("open");
    let print = |doc: &fitz::Document| {
        for l in doc.layers().unwrap() {
            println!(
                "{:>3} {}{:?} {} visible={} locked={}",
                l.index,
                "  ".repeat(l.depth as usize),
                l.kind,
                l.name,
                l.visible,
                l.locked
            );
        }
    };
    print(&doc);
    if let Some(name) = args.next() {
        let layer = doc
            .layers()
            .unwrap()
            .into_iter()
            .find(|l| l.name == name)
            .expect("no such layer");
        doc.set_layer(layer.index, !layer.visible).unwrap();
        println!("--- after toggling {name}");
        print(&doc);
    }
}
