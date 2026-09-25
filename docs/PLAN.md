# Plan de construcción por fases

Documento vivo. Cada fase tiene objetivo, entregables, tareas, criterios de aceptación medibles y riesgos. Una fase se cierra cuando **todos** sus criterios de aceptación se cumplen y están verificados en CI o con benchmark documentado. Las reglas generales están en [`AGENTS.md`](../AGENTS.md).

## Mapa general

```
F0 Fundamentos ─► F1 Visor mínimo ─► F2 Rendimiento en planos ─► F3 Lector completo ─┬─► F4 LaTeX ────────────┐
                                                                                      ├─► F5 3D ───────────────┼─► F7 Publicación 1.0
                                                                                      └─► F6 Escritura a mano ─┘
```

- F0–F3 son secuenciales: todo lo demás se apoya en el pipeline de render.
- F4, F5 y F6 son independientes entre sí y pueden avanzar en paralelo (distintas personas o agentes).
- La preparación de datos y modelo de F6 (Python) puede empezar en cualquier momento, incluso durante F1.

| Versión | Contenido |
|---|---|
| v0.1 | F0 + F1: abre y navega PDFs |
| v0.2 | F2: planos A0 fluidos, capas |
| v0.3 | F3: texto, búsqueda, índice, miniaturas, accesibilidad |
| v0.4 | F4: modo proyecto LaTeX con SyncTeX |
| v0.5 | F5: 3D (glTF + U3D) |
| v0.6 | F6: escritura a mano → LaTeX |
| v1.0 | F7: pulido, instaladores, publicación académica |

---

## F0 — Fundamentos

**Objetivo**: repo, toolchain y CI listos; riesgos técnicos principales descartados antes de escribir funcionalidades.

### Tareas

**Entorno (Windows)**
- [x] Instalar Rust (`rustup`, toolchain stable, target MSVC) y Visual Studio Build Tools con "Desktop development with C++".
- [x] Instalar LLVM/Clang (lo necesita `bindgen` para compilar `mupdf-sys`).
- [ ] Instalar `pnpm` (`corepack enable`), `cargo-nextest`, `cargo-deny`, `cargo-criterion`, Tauri CLI.
- [x] Confirmar WebView2 (viene con Windows 11).

**Repo**
- [ ] `LICENSE` (texto AGPL-3.0 completo), `README.md`, `CITATION.cff`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `CHANGELOG.md`, `.gitignore`, `.editorconfig`.
- [x] Workspace Cargo con los crates vacíos de la sección 3 de AGENTS.md (`doc`, `render`, `pdf3d`, `latex`, `ink`, `app`).
- [x] Frontend `ui/` con Vite + SolidJS + TypeScript strict + Biome.
- [x] Tauri 2 enlazado a `ui/`; ventana vacía que arranca.
- [x] `tauri-specta` generando los tipos TS de los comandos IPC.
- [x] `deny.toml` con la lista de licencias compatibles con AGPL.
- [ ] Crear repo público en GitHub y subir.

**CI (GitHub Actions)**
- [ ] Job de lint: `cargo fmt --check`, `clippy -D warnings`, `biome ci`.
- [ ] Job de test: `cargo nextest`, `vitest`.
- [ ] `cargo deny check`.
- [ ] Matriz de builds: Windows, macOS, Linux.
- [ ] Caché de `target/` y de pnpm.

**Corpus de benchmark** (`bench/corpus/`, solo material redistribuible, con `SOURCES.md` indicando origen y licencia de cada archivo)
- [ ] Paper LaTeX largo (≥ 50 páginas, con fórmulas y figuras vectoriales).
- [ ] Plano de arquitectura A0/A1 vectorial con capas OCG.
- [ ] PDF con anotación 3D U3D, otro con PRC y otro con glTF (si se encuentra).
- [ ] PDF escaneado grande (imágenes JBIG2/JPEG2000).
- [ ] PDF de ≥ 100 MB y ≥ 1000 páginas.
- [ ] Varios PDFs malformados.

**Spikes** (prototipos desechables en `spikes/`, cada uno con conclusión escrita en un ADR)
- [ ] **S1 — MuPDF multihilo**: con `mupdf-rs`, crear una display list de una página y rasterizar N tiles en paralelo desde varios hilos. Medir y comprobar si hay locks internos o crashes. Si la API segura no lo permite, decidir si usar `mupdf-sys` directamente con `fz_clone_context`.
- [ ] **S2 — Transporte de tiles**: enviar tiles RGBA de 512×512 de Rust a la webview por `tauri::ipc::Response` y por `tauri::ipc::Channel`; medir throughput (tiles/s) y latencia hasta que se pintan en canvas/WebGL.
- [ ] **S3 — Capas OCG**: comprobar qué expone `mupdf-rs` para listar y cambiar capas; si no lo expone, qué funciones de `mupdf-sys` hacen falta.

### Criterios de aceptación
- `pnpm tauri dev` abre una ventana en Windows; la CI pasa en las 3 plataformas.
- S1, S2 y S3 cerrados con un ADR cada uno, incluyendo números medidos.
- Si S2 muestra que la webview no alcanza ~60 tiles/s de 512×512, se reabre la decisión del viewport nativo con wgpu **antes** de empezar F1.

### Riesgos
- Compilar MuPDF en Windows/MSVC es la parte más frágil del build → documentar exactamente los pasos en `CONTRIBUTING.md`.
- `mupdf-rs` puede no cubrir toda la API que necesitamos → aceptar FFI directa encapsulada en `crates/render` y `crates/doc`.

---

## F1 — Visor mínimo (v0.1)

**Objetivo**: abrir un PDF y navegarlo con scroll y zoom sin bloqueos. Aquí se construye la arquitectura de render definitiva, no un prototipo.

### Tareas

**`crates/doc`**
- [ ] `Document::open(path)` → número de páginas, tamaño de cada página, metadatos. La apertura no puede interpretar todas las páginas.
- [ ] Soporte para PDFs protegidos con contraseña (la contraseña se pide en la UI y nunca se guarda).
- [ ] Errores tipados; un PDF corrupto nunca provoca un panic.

**`crates/render`**
- [ ] Trait `PageRenderer` + backend MuPDF.
- [ ] Modelo de tiles: `TileKey { doc, page, zoom_level, x, y }` y niveles de zoom discretos (potencias de √2).
- [ ] Planificador: cola de prioridad (visible > adyacente > precarga), cancelación por `generation` y pool `rayon`.
- [ ] Caché LRU de tiles con presupuesto de memoria.
- [ ] Display list por página, en la misma LRU.
- [ ] Spans de `tracing` en cada etapa (parse, display list, raster, envío).
- [ ] Benchmarks `criterion`: tiempo de apertura, tiempo hasta la primera página y tiles/s por documento del corpus.

**`crates/app`**
- [ ] Comandos: `open_document`, `set_viewport` (el frontend envía el viewport y el backend decide qué tiles hacen falta) y `close_document`.
- [ ] Canal (`Channel`) por documento para empujar tiles en cuanto están listos.

**`ui/src/viewer`**
- [ ] Layout continuo vertical con la altura de cada página conocida de antemano (sin saltos de scroll).
- [ ] Compositor: pinta los tiles disponibles y, mientras llegan los nítidos, el mejor nivel que tenga (escalado).
- [ ] Zoom con rueda+Ctrl, pinch en trackpad y atajos; zoom anclado al cursor.
- [ ] Modos de ajuste: ancho de página, página completa y 100 %.
- [ ] Abrir archivo: diálogo, arrastrar y soltar, y argumento de línea de comandos.
- [ ] Recordar la última posición en cada documento.

### Criterios de aceptación
- Primera página visible < 150 ms en el PDF de 100 MB (medido con `tracing` y registrado).
- Scroll continuo en el paper de 50 páginas: 60 fps sin huecos blancos visibles (se verifica con una grabación de rendimiento en DevTools).
- Cambiar el zoom rápidamente 20 veces no deja trabajo zombie: la cola se vacía en < 200 ms tras parar.
- Todos los PDFs malformados del corpus se abren o fallan con un mensaje de error, sin crash.

---

## F2 — Rendimiento en planos (v0.2)

**Objetivo**: los planos A0 con millones de trazos se sienten tan fluidos como un PDF de texto.

### Tareas
- [ ] Pirámide completa de zoom sobre display lists; nunca rasterizar una página entera a zoom alto.
- [ ] Construcción de la display list en segundo plano mientras se muestra una miniatura de baja resolución.
- [ ] Caché en disco opcional para los tiles de documentos grandes, con clave `hash(archivo)+nivel+tile` y límite de tamaño.
- [ ] Capas OCG: panel de capas, activar/desactivar e invalidación selectiva de display lists y tiles.
- [ ] Pan con inercia; mientras hay movimiento se priorizan los tiles en la dirección del desplazamiento.
- [ ] Herramienta de medida (distancia/área) usando la escala del documento si viene definida (diccionarios `Measure`/`Viewport` de PDF).
- [ ] Minimapa de navegación para planos.
- [ ] Perfilado con `tracy` o `puffin` sobre el plano del corpus; optimizar los 3 puntos más costosos.

### Criterios de aceptación
- Zoom en el plano A0: respuesta visual < 16 ms y tiles nítidos < 150 ms en hardware de referencia (documentado en `bench/HARDWARE.md`).
- Activar/desactivar una capa: repintado completo del viewport < 300 ms.
- La memoria no supera el presupuesto configurado tras 5 minutos de navegación continua por el plano.
- Informe comparativo frente a otros visores (SumatraPDF, Okular, Acrobat Reader) con la misma métrica.

---

## F3 — Lector completo (v0.3)

**Objetivo**: funciones básicas de cualquier buen lector, con accesibilidad desde el principio.

### Tareas
- [ ] Extracción de texto estructurado (`stext` de MuPDF) en `crates/doc`, de forma perezosa y por página.
- [ ] Capa de texto transparente superpuesta en el visor: selección, copiar y accesible para lectores de pantalla.
- [ ] Búsqueda incremental por todo el documento en segundo plano, con resultados en streaming y resaltado.
- [ ] Índice (outline), enlaces internos y externos (los externos piden confirmación) e historial de navegación atrás/adelante.
- [ ] Panel de miniaturas virtualizado (solo se renderizan las visibles, con un nivel de zoom propio).
- [ ] Paleta de comandos (`Ctrl+K`) y atajos configurables.
- [ ] Pestañas o múltiples documentos abiertos.
- [ ] Tema claro/oscuro, modo lectura nocturna e i18n en español e inglés.
- [ ] Anotaciones básicas (resaltado, nota) guardadas en el propio PDF mediante MuPDF.
- [ ] Recarga automática cuando el archivo cambia en disco (base de F4).

### Criterios de aceptación
- Búsqueda en el PDF de 1000 páginas: primeros resultados < 200 ms y búsqueda completa sin bloquear la UI.
- Auditoría de accesibilidad WCAG 2.1 AA: navegación completa por teclado y texto legible con NVDA en Windows.
- Tests e2e (Playwright) de los flujos abrir → buscar → saltar a resultado → seleccionar → copiar.

---

## F4 — LaTeX como fuente principal (v0.4)

**Objetivo**: ciclo editar → compilar → ver → saltar entre fuente y PDF, integrado y rápido.

### Tareas

**`crates/latex`**
- [ ] Detectar el proyecto: `.tex` raíz, `% !TEX root` y `latexmkrc`.
- [ ] Compilación con Tectonic como librería, en segundo plano y cancelable si llega un guardado nuevo.
- [ ] Fallback a `latexmk` del sistema (TeX Live/MiKTeX), configurable por proyecto.
- [ ] Parser de logs: errores y warnings con archivo/línea, con tests sobre logs reales.
- [ ] Parser de SyncTeX (`.synctex.gz`) con búsqueda en ambas direcciones y fuzzing.
- [ ] `--shell-escape` solo con confirmación explícita por proyecto.

**UI**
- [ ] Vista dividida editor | PDF, redimensionable.
- [ ] CodeMirror 6: resaltado LaTeX, autocompletado de comandos, `\ref`/`\cite` (leyendo `.aux` y `.bib`), plegado y diagnósticos inline.
- [ ] Previsualización KaTeX de la fórmula bajo el cursor.
- [ ] SyncTeX: Ctrl+clic en el PDF salta a la fuente; comando "ir al PDF" desde la fuente; resaltado temporal.
- [ ] Recarga del PDF conservando posición e invalidando solo las páginas cuyo contenido cambió (comparar hashes de los content streams).
- [ ] Árbol de archivos del proyecto.

### Criterios de aceptación
- Guardar un cambio en el paper de 50 páginas actualiza el PDF sin salto de scroll; el tiempo total queda dominado por la compilación, con el overhead del visor < 100 ms.
- SyncTeX correcto en ≥ 95 % de un conjunto de 100 puntos de prueba anotados.
- Un error de compilación se ve en el editor en la línea correcta.

### Riesgos
- Tectonic no soporta algunos paquetes o motores (p. ej. LuaLaTeX) → el fallback a la toolchain del sistema es obligatorio, no opcional.

---

## F5 — 3D en PDF (v0.5)

**Objetivo**: ver e interactuar con el contenido 3D embebido en PDFs.

### Tareas

**`crates/pdf3d`**
- [ ] Localizar las anotaciones `/3D` y `/RichMedia` y extraer el stream 3D y su subtipo (`U3D`, `PRC`, glTF) mediante la API de objetos PDF de MuPDF.
- [ ] Tipo `Scene` común: nodos, mallas, materiales, vistas `3DView` (cámara, fondo, modo de render, visibilidad de nodos) e iluminación.
- [ ] Importador glTF (crate `gltf`).
- [ ] Parser U3D (ECMA-363): bloques, CLOD mesh y materiales, con tests y `cargo fuzz`.
- [ ] Parser PRC (ISO 14739-1), solo teselado; el B-rep queda fuera de alcance salvo ADR nuevo.
- [ ] Serialización binaria de la escena para el frontend.

**UI**
- [ ] Visor three.js (WebGPURenderer con fallback a WebGL2) incrustado en la posición de la anotación, o a pantalla completa.
- [ ] Controles de órbita, pan y zoom; selector de vistas predefinidas; árbol de nodos con visibilidad; cortes (clipping planes); modo alámbrico.
- [ ] Mostrar el póster 2D de la anotación hasta que el usuario activa el 3D (carga perezosa).

### Criterios de aceptación
- Los PDFs 3D del corpus se muestran con la geometría correcta frente a una captura de referencia de Acrobat.
- Un modelo de 1 M de triángulos a ≥ 60 fps en hardware de referencia.
- Fuzzing de 1 h sin crashes en los parsers U3D y PRC.

### Riesgos
- U3D y PRC tienen especificaciones largas y ficheros reales que no las cumplen al pie de la letra → priorizar según los ficheros reales que encontremos, no la especificación completa.

---

## F6 — Escritura a mano → LaTeX (v0.6)

**Objetivo**: dibujar una fórmula y obtener LaTeX correcto en < 300 ms, en local.

### Tareas

**Datos y modelo** (`models/training/`, Python + PyTorch, con `uv`)
- [ ] Revisar las licencias de MathWriting, CROHME y otros datasets, y documentarlas.
- [ ] Baseline de fase 1: evaluar 2–3 modelos imagen→LaTeX preentrenados sobre trazos rasterizados (ExpRate, CER y latencia en CPU) → ADR con la elección.
- [ ] Exportación a ONNX + cuantización INT8 + verificación de equivalencia numérica.
- [ ] Fase 2: modelo online basado en trazos (encoder sobre secuencias `(x, y, t, p)` + decoder de tokens LaTeX), entrenado con MathWriting y afinado con CROHME. Comparar con el baseline.
- [ ] Normalización de LaTeX para la evaluación (igualdad tras tokenizar y canonicalizar).
- [ ] Tarjeta del modelo (licencia, datos, métricas, latencia).

**`crates/ink`**
- [ ] Tipos de trazo, normalización (escala, re-muestreo, suavizado) y rasterizado idéntico al del entrenamiento (con test de paridad frente a Python).
- [ ] Inferencia con `ort`, beam search y top-k.
- [ ] Postprocesado y validación de los candidatos.

**UI (`ui/src/ink`)**
- [ ] Lienzo con Pointer Events (presión, eventos coalescidos y predichos), deshacer y borrador.
- [ ] Reconocimiento en vivo con debounce mientras se escribe.
- [ ] Top-k candidatos renderizados con KaTeX; insertar en el editor LaTeX en la posición del cursor.
- [ ] Opt-in para guardar las correcciones del usuario como datos de ajuste (local, exportable).

### Criterios de aceptación
- ExpRate del modelo elegido reportado sobre el test de CROHME y el de MathWriting, reproducible con un comando.
- Latencia p95 < 300 ms en CPU en hardware de referencia.
- Test de regresión de exactitud en CI sobre un conjunto fijo pequeño.

---

## F7 — Pulido y publicación 1.0

### Tareas
- [ ] Instaladores: MSI/NSIS (Windows), DMG (macOS), AppImage/deb (Linux), generados por CI en cada tag.
- [ ] Auto-actualización con Tauri updater y firmas.
- [ ] Revisión completa de accesibilidad y de rendimiento frente a los presupuestos de AGENTS.md.
- [ ] Documentación de usuario (español/inglés) y documentación de arquitectura.
- [ ] Web del proyecto (GitHub Pages) con descargas y demo en vídeo.
- [ ] `CITATION.cff` con la versión final; DOI con Zenodo en cada release.
- [ ] Informe/artículo técnico: arquitectura de render, benchmarks comparativos y evaluación del reconocimiento de escritura.

### Criterios de aceptación
- Todos los presupuestos de rendimiento de AGENTS.md verificados y publicados en `bench/RESULTS.md`.
- Instalación limpia y funcional en las 3 plataformas.
- Release v1.0 con DOI.
