# PDF Reader

A desktop PDF reader built to be as fast as possible on:

1. **Vector-heavy documents** — architecture plans (A0/A1, millions of paths, OCG layers) and technical drawings.
2. **PDFs with 3D content** — U3D, PRC and glTF annotations, which almost no viewer besides Acrobat renders.
3. **LaTeX as the primary source** — open a `.tex` project, compile, view the PDF and jump between source and PDF with SyncTeX.
4. **Handwriting → LaTeX** — draw a formula with mouse, pen or tablet and get its LaTeX code, fully offline.

> **Status:** early development (phase F0, foundations). Nothing is usable yet. See [`docs/PLAN.md`](docs/PLAN.md).

## Stack

Rust core (workspace in `crates/`), Tauri 2 desktop shell, SolidJS + TypeScript UI (`ui/`), MuPDF for rendering, three.js for 3D, Tectonic for LaTeX and ONNX Runtime for handwriting recognition. Architecture and conventions are described in [`AGENTS.md`](AGENTS.md); decisions are recorded in [`docs/adr/`](docs/adr/).

## Building

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the toolchain setup (Windows first), then:

```bash
pnpm install
pnpm tauri dev
```

## License

[AGPL-3.0-or-later](LICENSE). This is required to distribute the application together with MuPDF (see [ADR 0001](docs/adr/0001-mupdf-agpl.md)). If you use this project in academic work, please cite it using [`CITATION.cff`](CITATION.cff).

---

## Resumen en español

Lector de PDF de escritorio orientado al rendimiento, pensado para planos de arquitectura con millones de vectores y capas, PDFs con contenido 3D (U3D, PRC, glTF), proyectos LaTeX con SyncTeX bidireccional y reconocimiento de fórmulas escritas a mano a LaTeX, todo en local. Núcleo en Rust, interfaz con Tauri 2 y SolidJS, render con MuPDF. Licencia AGPL-3.0-or-later. El plan por fases está en [`docs/PLAN.md`](docs/PLAN.md) y las normas de trabajo en [`AGENTS.md`](AGENTS.md).
