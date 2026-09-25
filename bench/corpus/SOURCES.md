# Benchmark corpus — sources and licenses

Only redistributable documents belong here; never private documents (AGENTS.md §13). PDFs are **not committed**:

- `generated/`: synthetic and deterministic, `cargo run -p corpusgen --release`.
- `real/`: real-world files, downloaded and verified by SHA-256 with `node bench/tools/fetch-corpus.mjs`.
  The machine-readable list (full hashes) is [`sources.json`](sources.json).
- `fixtures/`: small synthetic files used by unit tests; these *are* committed.

`pnpm corpus` does both.

## Synthetic (`generated/`)

| File | Purpose |
|---|---|
| `paper-60.pdf` | 60-page paper: text in Helvetica and vector plots |
| `big-1000p.pdf` | 1000 pages, 123 MB, one incompressible image per page |
| `plan-a0-layers.pdf` | A0 plan: ~1 M path segments, 5 000 labels, 5 OCG layers |
| `encrypted-secret.pdf` | AES-256 encrypted copy of `paper-60.pdf`, password `secret` |
| `malformed/*.pdf` | Empty, garbage, truncated, broken xref/trailer, wrong lengths, cyclic page tree, huge page count, zero MediaBox, deep nesting |

## Real-world (`real/`)

| File | Purpose | Source | License | SHA-256 | Size |
|---|---|---|---|---|---|
| `pgfmanual-3.1.12.pdf` | Long LaTeX document (1324 pages): formulas, vector figures, many links | [link](https://github.com/pgf-tikz/pgf/releases/download/3.1.12/pgfmanual-3.1.12.pdf) | GFDL-1.2 OR LPPL-1.3c (PGF/TikZ manual, Till Tantau et al.) | `32cef61a31617547…` | 9.5 MB |
| `ID_Carey_20200331_TM_geo.pdf` | Large vector map (US Topo GeoPDF) with 30 OCG layers | [link](https://prd-tnm.s3.amazonaws.com/StagedProducts/Maps/USTopo/PDF/ID/ID_Carey_20200331_TM_geo.pdf) | Public domain (U.S. Geological Survey, U.S. Government work) | `f5c3250ec2ddbeb5…` | 34.8 MB |
| `media9.pdf` | 3D annotations: U3D and PRC models (media9 package manual, v1.30) | [link](https://mirrors.ctan.org/macros/latex/contrib/media9/doc/media9.pdf) | LPPL-1.3c (media9 package, Alexander Grahn) | `d6f0172dbd0156fd…` | 3.8 MB |
| `originofspecies00darwuoft.pdf` | Large scan (568 pages) with JBIG2 and JPEG 2000 images | [link](https://archive.org/download/originofspecies00darwuoft/originofspecies00darwuoft.pdf) | Public domain (Darwin, 1909 Collier edition; Internet Archive: NOT_IN_COPYRIGHT) | `6f04f39b2a590a78…` | 25.4 MB |

Still missing: a real A0/A1 architecture plan with layers, a PDF 2.0 file with glTF 3D content, and a
real document of at least 100 MB and 1000 pages (covered synthetically by `big-1000p.pdf`).
