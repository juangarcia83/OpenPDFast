# ADR 0001 — MuPDF as render backend, AGPL-3.0-or-later license

- Status: accepted
- Date: 2026-09-25

## Context

Rendering speed on vector-heavy documents (architecture plans) is the top priority. Candidates were PDFium (BSD), MuPDF (AGPL-3.0 or commercial) and hayro (pure Rust, young).

## Decision

- Use **MuPDF** through the `mupdf` Rust crate, behind the `PageRenderer` trait.
- Publish the project on GitHub as open source under **AGPL-3.0-or-later**, which is required to distribute it together with MuPDF. The project is academic, so a commercial MuPDF license is not needed.

## Consequences

- Each page can be interpreted once into a display list and rasterized many times (tiles, zoom levels, threads), which is the main performance advantage for plans.
- Every dependency must be AGPL-compatible; enforced by `cargo deny`.
- Anyone distributing a modified version, including as a network service, must publish its source.
- `PageRenderer` stays in place so other backends can still be benchmarked, but they are not planned to ship.
