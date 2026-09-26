# AGENTS.md — OpenPDFast (lector PDF + LaTeX)

Guía para agentes (Claude Code, Codex, etc.) que trabajen en este repositorio. Léela entera antes de tocar código.

## 1. Qué construimos

Un lector de PDF de escritorio **lo más rápido posible**, pensado para:

1. **Documentos pesados en vectores**: planos de arquitectura (A0/A1, millones de trazos, capas OCG), esquemas técnicos.
2. **PDFs con contenido 3D** (anotaciones 3D: U3D, PRC y glTF), que casi ningún visor fuera de Acrobat renderiza.
3. **LaTeX como fuente principal**: abrir un proyecto `.tex`, compilar, ver el PDF y saltar entre fuente y PDF (SyncTeX) en ambas direcciones.
4. **Escritura a mano → LaTeX**: dibujar una fórmula con ratón/lápiz/tableta y obtener su código LaTeX, insertable en la fuente.

Prioridad cuando haya conflicto: **rendimiento percibido > fidelidad de render > funcionalidades nuevas**.

El plan por fases está en [`docs/PLAN.md`](docs/PLAN.md). Antes de empezar una tarea, localiza su fase y respeta sus criterios de aceptación; al terminarla, marca su casilla.

## 2. Stack

| Capa | Elección | Por qué |
|---|---|---|
| Núcleo | **Rust** (stable, edition 2024), workspace de crates | Rendimiento, paralelismo seguro, sin GC |
| Shell de escritorio | **Tauri 2** | Binario pequeño, IPC binario, multiplataforma (Windows primero) |
| UI | **TypeScript + SolidJS + Vite** | Reactividad fina sin VDOM: la UI no compite con el render |
| Render PDF | **MuPDF** vía `mupdf-sys` y nuestra capa segura `crates/fitz` (ADR 0004), detrás del trait `PageRenderer` | El más rápido en documentos vectoriales; display lists reutilizables entre tiles; AGPL compatible con nuestra licencia |
| 3D | Parser propio en Rust → malla normalizada → **three.js (WebGPURenderer, fallback WebGL2)** | |
| LaTeX | **Tectonic** embebido; fallback a `latexmk` del sistema si hay TeX Live/MiKTeX | Compilación sin instalación externa |
| Editor | **CodeMirror 6** + gramática LaTeX | Ligero, extensible |
| Previsualización matemática | **KaTeX** | Rápido; además sirve de validador sintáctico |
| Inferencia ML | **ONNX Runtime** vía crate `ort`, local | Offline, sin enviar datos a terceros |
| Paquetes | `cargo` + `pnpm` | |

No introducir frameworks alternativos (React, Electron, egui…) sin una decisión registrada en `docs/adr/`.

## 3. Estructura del repo

```
crates/
  fitz/       # Capa segura propia sobre mupdf-sys (ADR 0004); único crate con unsafe de MuPDF
  doc/        # Abstracción de documento: abrir, metadatos, outline, texto, enlaces, OCG
  render/     # Trait PageRenderer, backends, teselado, caché de tiles, planificador
  pdf3d/      # Extracción de streams 3D (U3D, PRC, glTF) → Mesh normalizada + vistas 3DView
  latex/      # Compilación (Tectonic/latexmk), parseo de logs, SyncTeX
  ink/        # Modelo de trazos, preprocesado, inferencia ONNX, postprocesado LaTeX
  app/        # Binario Tauri: comandos IPC, estado, ventanas
ui/           # Frontend SolidJS
  src/viewer/   # Canvas de páginas, compositor de tiles
  src/editor/   # CodeMirror + integración SyncTeX
  src/three/    # Visor 3D
  src/ink/      # Lienzo de escritura a mano
  src/design/   # Tokens y componentes base
models/       # Modelos .onnx (Git LFS) + tarjetas del modelo
bench/        # Corpus de PDFs de referencia y benchmarks
docs/adr/     # Decisiones de arquitectura (una por archivo)
```

Los crates `fitz`, `doc`, `render`, `pdf3d`, `latex` e `ink` **no dependen de Tauri** y deben poder usarse desde CLI/tests.

## 4. Arquitectura de render (lo más importante)

- **Nunca bloquear el hilo de UI ni el hilo principal de Tauri.** Todo render ocurre en un pool (`rayon`) gestionado por el planificador de `render`.
- **Teselado**: las páginas se renderizan en tiles de 256–512 px por nivel de zoom (pirámide tipo mapa). Un plano A0 a 800 % nunca se rasteriza entero.
- **Progresivo**: primero un bitmap de baja resolución (o el nivel de zoom anterior escalado), luego los tiles nítidos. Al hacer zoom se muestra el nivel previo escalado de inmediato.
- **Prioridad y cancelación**: tiles visibles > adyacentes al viewport > precarga. Cada petición lleva un `generation`; cuando el viewport cambia, las peticiones obsoletas se cancelan antes de empezar.
- **Caché LRU** de tiles con presupuesto de memoria configurable (por defecto 1 GB), más caché en disco opcional para documentos grandes reabiertos.
- **Transporte**: tiles como bytes RGBA crudos por `tauri::ipc::Response` (sin JSON, sin PNG), subidos como textura / `ImageBitmap` en el frontend. No codificar/decodificar imágenes en el camino caliente.
- **Documentos pesados en vectores**: interpretar cada página **una sola vez** a un `fz_display_list` de MuPDF y rasterizar todos los tiles y niveles de zoom desde esa display list (en paralelo, un `fz_context` clonado por hilo). Las display lists entran en la misma LRU con su propio presupuesto. Respetar capas OCG: activar/desactivar capas es un caso de uso central para planos (invalida display lists de la página afectada).
- **Texto y selección** se extraen aparte (crate `doc`) y se superponen como capa HTML/SVG transparente; no se re-renderiza para seleccionar.

### Presupuestos de rendimiento (son requisitos, no deseos)

| Métrica | Objetivo |
|---|---|
| Primera página visible (PDF de 100 MB) | < 150 ms desde abrir |
| Scroll / pan | 60 fps sostenidos (120 en pantallas de alta frecuencia) sin huecos blancos visibles > 1 frame en el caso normal |
| Zoom en plano A0 | Feedback inmediato (< 16 ms) + tiles nítidos < 150 ms |
| Recompilación LaTeX → PDF actualizado | Recarga conservando posición de scroll; solo se invalidan páginas cambiadas |
| Arranque en frío de la app | < 500 ms hasta ventana interactiva |
| Escritura a mano → candidatos LaTeX | < 300 ms en CPU para una fórmula típica |

Cualquier cambio en `crates/render`, `crates/doc` o `ui/src/viewer` **debe** ejecutar `cargo bench -p render` (corpus en `bench/corpus/`) e incluir los números antes/después en la descripción del cambio. Regresiones > 5 % requieren justificación explícita.

## 5. 3D en PDF

- Formatos: **U3D** (ECMA-363), **PRC** (ISO 14739-1) y **glTF** (extensión de PDF 2.0). Orden de implementación: glTF → U3D → PRC teselado → PRC B-rep (este último solo si hay demanda real).
- `pdf3d` produce un tipo `Scene` propio (mallas, materiales, nodos, vistas predefinidas de los diccionarios `3DView`, iluminación). El frontend no conoce los formatos de origen.
- Mallas enviadas como buffers binarios (posiciones/normales/índices) listos para `BufferGeometry`, sin conversión en JS.
- Parsers robustos ante datos corruptos: nunca `panic`, siempre `Result`; fuzzing con `cargo fuzz` para U3D y PRC.

## 6. LaTeX como fuente principal

- Modo proyecto: se abre una carpeta con un `.tex` raíz (detectar `% !TEX root` y `\documentclass`).
- Compilación incremental en segundo plano al guardar (debounce configurable); errores del log mapeados a línea/columna y mostrados en el editor.
- **SyncTeX bidireccional**: clic en el PDF → línea de la fuente; cursor en la fuente → resaltado en el PDF. El parseo de `.synctex.gz` vive en `crates/latex`, con tests.
- Tectonic por defecto; si el proyecto necesita `--shell-escape` (minted, etc.) o paquetes no disponibles, usar la toolchain del sistema y avisar. Nunca activar shell-escape sin confirmación del usuario.

## 7. Escritura a mano → LaTeX

- **Captura**: Pointer Events con presión, `getCoalescedEvents()` y `getPredictedEvents()` para baja latencia. Cada trazo se guarda como secuencia `(x, y, t, pressure)`; no solo como imagen.
- **Modelo (fase 1)**: modelo imagen→LaTeX preentrenado exportado a ONNX, sobre los trazos rasterizados.
- **Modelo (fase 2)**: modelo *online* basado en trazos, entrenado/afinado con **MathWriting** y **CROHME**. Scripts de entrenamiento en `models/training/` (Python, PyTorch), fuera del binario.
- **Postprocesado**: normalizar el LaTeX, validar con KaTeX, devolver top-k candidatos con su render para que el usuario elija.
- Las correcciones del usuario pueden guardarse como datos de ajuste **solo con opt-in explícito**.
- Un proveedor en la nube (p. ej. API de Claude con visión) puede existir como backend opcional detrás del mismo trait, **desactivado por defecto**.
- Cada modelo en `models/` lleva una tarjeta: origen, licencia, datos, métricas (ExpRate, CER), latencia medida.

## 8. Diseño de interfaz

- **Centrada en el documento**: el lienzo ocupa todo; los paneles (miniaturas/índice, editor LaTeX, inspector 3D, capas) son plegables y recuerdan su estado.
- **Teclado primero**: paleta de comandos (`Ctrl+K`), todos los comandos accesibles por teclado, atajos configurables.
- Tema claro y oscuro; en oscuro el PDF no se invierte por defecto (opción aparte de "modo lectura nocturna").
- Tokens de diseño (color, espaciado, tipografía, radios) como variables CSS en `ui/src/design/tokens.css`. Nada de colores o tamaños hardcodeados en componentes.
- Tipografía de UI: Inter; monoespaciada: JetBrains Mono.
- Animaciones solo con `transform`/`opacity`, ≤ 150 ms, y respetando `prefers-reduced-motion`.
- Accesibilidad: contraste WCAG AA, foco visible, capa de texto del PDF accesible a lectores de pantalla.
- Interfaz en español e inglés desde el principio (i18n con claves, sin cadenas sueltas en componentes).

## 9. Convenciones de código

**Rust**
- `cargo fmt` y `cargo clippy --all-targets -- -D warnings` limpios.
- Librerías: errores con `thiserror`, sin `unwrap`/`expect` fuera de tests. Binario `app`: `anyhow` permitido.
- Logs con `tracing` (spans en render, compilación e inferencia para poder perfilar).
- `unsafe` solo en FFI, encapsulado y con comentario `// SAFETY:`.
- Dependencias nuevas: justificar en el cambio; `cargo deny check` debe pasar (licencias y advisories). Solo licencias compatibles con **AGPL-3.0-or-later** (MIT, Apache-2.0, BSD, MPL-2.0, LGPL, GPL-3.0, AGPL-3.0…). Nada de GPL-2.0-only ni licencias propietarias.

**TypeScript**
- `strict: true`. Lint y formato con **Biome**.
- Los tipos de IPC se generan desde Rust (`specta`/`tauri-specta`); no escribir a mano tipos que dupliquen structs de Rust.
- Sin lógica pesada en el frontend: si algo es costoso, va a Rust.

**General**
- Identificadores, comentarios de código y mensajes de commit en inglés; documentación de producto puede estar en español.
- Commits con Conventional Commits (`feat(render): …`, `fix(latex): …`).
- Un cambio = un propósito. No mezclar refactors con features.

## 10. Comandos

```bash
pnpm install                 # dependencias del frontend
pnpm tauri dev               # app en desarrollo
cargo nextest run --workspace
pnpm test                    # vitest
pnpm test:e2e                # Playwright contra la app (tauri-driver)
cargo bench -p render        # benchmarks de render sobre bench/corpus
cargo deny check
```

(Se crearán al hacer el scaffolding; si alguno no existe todavía, créalo siguiendo este documento.)

## 11. Tests

- Tests de render por **comparación de imagen** contra referencias en `bench/corpus/golden/` (tolerancia perceptual, no igualdad exacta de bytes).
- Corpus mínimo: paper LaTeX largo, plano A0 vectorial con capas, PDF con U3D, PDF con PRC, PDF escaneado, PDF corrupto/malformado.
- Los PDFs del corpus **no se suben al repo** (GitHub no admite archivos > 100 MB y el repo debe seguir siendo ligero): los sintéticos se generan de forma determinista (`cargo run -p corpusgen --release`) y los reales se descargan y verifican por SHA-256 desde `bench/corpus/sources.json` (`node bench/tools/fetch-corpus.mjs`). Solo se versionan los *fixtures* pequeños de `bench/corpus/fixtures/`.
- Parsers (`pdf3d`, SyncTeX) con fuzzing y tests de propiedades (`proptest`).
- `ink`: test de regresión de exactitud sobre un conjunto fijo de fórmulas manuscritas.

## 12. Reglas para agentes

- Antes de optimizar, **mide** (benchmarks o `tracing`); incluye los números.
- No bloquees el hilo principal; no añadas `sleep`/polling para sincronizar.
- No subas PDFs del usuario, fuentes LaTeX ni trazos manuscritos a ningún servicio externo.
- No cambies el stack de la sección 2 ni los presupuestos de la sección 4 sin un ADR en `docs/adr/`.
- Si una tarea toca varias crates, empieza por los tipos/traits compartidos y sus tests.
- Si algo de este documento está desactualizado respecto al código, dilo y propón la corrección.

## 13. Proyecto abierto y académico

- **Licencia: AGPL-3.0-or-later** (obligatoria al distribuir con MuPDF). Todo el código fuente es público en GitHub; los binarios publicados deben poder reconstruirse desde el repo.
- Cada archivo fuente nuevo lleva cabecera SPDX: `// SPDX-License-Identifier: AGPL-3.0-or-later`.
- La UI incluye un enlace visible al código fuente (pantalla "Acerca de"), como exige la AGPL.
- Ficheros de repo que deben existir y mantenerse: `LICENSE` (texto AGPL-3.0 completo), `README.md` (en inglés, con resumen en español), `CITATION.cff`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `CHANGELOG.md`.
- **Modelos y datasets**: registrar la licencia de cada peso y dataset en su tarjeta. Algunos datasets de escritura matemática tienen licencias no comerciales (NC): son válidos para uso académico, pero la tarjeta debe indicarlo y los pesos derivados se publican con esa misma restricción, separados del código.
- **Reproducibilidad**: los resultados que aparezcan en publicaciones (benchmarks de render, ExpRate/CER del modelo de escritura) deben poder regenerarse con un comando documentado, con versión de corpus/modelo y hardware anotados.
- **Privacidad**: el corpus de `bench/` solo contiene PDFs con licencia que permita redistribuirlos (origen, licencia y hash en `bench/corpus/SOURCES.md`); nunca documentos privados.
- CI en GitHub Actions: fmt, clippy, tests, `cargo deny`, Biome y builds para Windows, macOS y Linux.

## 14. Decisiones abiertas

1. **Viewport nativo con wgpu** en lugar de canvas en webview. S2 (ADR 0003) mide ~170 tiles/s pintados, 3× el umbral: no hace falta para F1; se revisa en F2 solo si los planos lo exigen.
2. Modelo concreto de la fase 1 de escritura a mano (evaluar ExpRate y latencia en CPU).

Decisiones cerradas: backend de render = MuPDF; licencia = AGPL-3.0-or-later (ver `docs/adr/0001-mupdf-agpl.md`).
