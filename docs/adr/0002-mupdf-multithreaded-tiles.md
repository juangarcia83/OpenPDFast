# ADR 0002 — Multithreaded tile rendering with MuPDF (spike S1)

- Status: accepted
- Date: 2026-09-25
- Spike: `spikes/s1-mupdf-threads` (`s1-mupdf-threads`, `s1b-raw-context`)
- Hardware: see [`bench/HARDWARE.md`](../../bench/HARDWARE.md) (Ryzen 5 3600, 6C/12T). The machine had other
  applications running (~39 % CPU): absolute numbers are noisy, ratios are consistent across runs.

## Question

Can one MuPDF display list be rasterized into 512 px tiles from many threads at once, safely and with a real
speed-up? Corpus: `bench/corpus/generated/` (`cargo run -p corpusgen --release`), mainly `plan-a0-layers.pdf`
(A0, ~1 M path segments, 5 OCG layers, 5 000 text labels) and `paper-60.pdf`.

## Results

**Display lists are mandatory.** Re-running the page for each tile vs. rasterizing from one display list
(A0 plan, 1 thread): 1 792 → 126 ms/tile at scale 1; 2 197 → 75 ms/tile at scale 2. Building the display list
costs ~0.9 s for the plan and < 1 ms for a paper page.

**Safety: fine.** mupdf-rs clones one `fz_context` per thread from a base context with real locks, and
`DisplayList` is `Send + Sync`. Tile outputs are bit-identical across 1–12 threads (hashed).

**Scaling: poor on heavy pages, good on light ones.**

| Tiles/s (scale 2, 140 tiles) | 1 thr | 2 thr | 4 thr | 8 thr | 12 thr |
|---|---|---|---|---|---|
| Paper page (scale 4, 35 tiles) | 1 596 | 2 745 | 4 510 | 4 495 | 4 998 |
| A0 plan, one display list | 13.4 | 22.4 | 20.7 | 6.8 | 5.3 |
| A0 walls only, one display list | 70 | 74 | 41 | 23 | 16 |
| A0 walls only, **per-cell display lists** | 181 | 342 | 620 | — | 596 |

Root cause, measured with instrumented locks (`s1b-raw-context`):

1. **Every tile walks the whole display list.** MuPDF decodes the state of every node (keeping/dropping
   refcounted stroke states, colorspaces, paths) before culling it by area, so the cost of a tile is
   O(nodes on the page), not O(visible nodes): ~400 k lock operations per tile on the walls layer alone.
2. Those refcounts and every `fz_malloc` take the global `FZ_LOCK_ALLOC`, and glyph rendering takes
   `FZ_LOCK_GLYPHCACHE`/`FZ_LOCK_FREETYPE`. With many threads they all contend on the same locks
   (e.g. 5.7 M contended acquisitions out of 60 M at 12 threads).
3. Replacing the allocator (mimalloc) or the lock implementation (parking_lot) does **not** help: the
   number of lock operations is the problem, not their cost.

**Spatial partitioning fixes it.** Recording one sub-display-list per 512 pt cell (by running the master list
into a display-list device with the cell as area, which culls) makes a tile touch only its cell's nodes:
2–3× faster single-threaded and near-linear scaling up to the physical cores (596–620 vs 16 tiles/s at
12 threads, ~30×), with bit-identical output. Partitioning the whole plan costs ~0.37 s (walls) to ~1.1 s
(full plan), off the critical path.

**Large text objects are a separate pathology.** The generated plan draws its 5 000 labels inside one
`BT … ET`; MuPDF turns that into a single text node covering the whole page, so every tile (and every cell)
renders every glyph (~26 k FreeType lock acquisitions per tile, 120 ms/tile). Real CAD exports do this too.
Fix (F2): when partitioning, split text nodes per cell (a filtering device that keeps only the spans/glyphs
intersecting the cell).

## Decision

- Interpret each page once into a display list and rasterize every tile from it (as AGENTS.md §4 says).
- Rasterize on a pool of `min(physical cores, 4)` threads for now; raise it once partitioning lands.
- Heavy pages (display list build > ~50 ms) get **per-cell sub-lists** built in the background after the
  master list; tiles use the cell list when available and the master list meanwhile. Cell size is a tuning
  parameter (start at 512 pt). Text-node splitting comes with F2.
- Prefer tiles of **512 px**: per-tile overhead on heavy pages is proportional to the nodes visited, so fewer,
  larger tiles win; S2 shows transport handles them.
- Keep MuPDF's default allocator and locks; revisit only with new measurements.
- MuPDF must be built with the `base14-fonts` feature (non-embedded Helvetica/Times are very common).
