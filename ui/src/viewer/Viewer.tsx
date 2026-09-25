// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Continuous vertical viewer. Pages are absolutely positioned divs (only the
// ones near the viewport are mounted); tiles are canvases positioned in
// percent of their page, so a zoom change rescales what is on screen in the
// same frame while sharper tiles are requested. Scrolling is native, so the
// browser compositor keeps it smooth without JavaScript per frame.

import {
  createEffect,
  createMemo,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
  untrack,
} from "solid-js";
import { commands, type FitMode, type OpenedDocument, type Viewport } from "../bindings";
import { t } from "../i18n";
import {
  clampZoom,
  computeLayout,
  fitZoom,
  levelFor,
  levelScale,
  pageAt,
  pagesBetween,
  regionsIn,
  TILE_SIZE,
  type ViewRect,
  viewportRegions,
} from "./layout";
import type { ChannelMessage } from "./protocol";
import { createTile, TileStore, tileKey } from "./tiles";
import "./viewer.css";

/** GPU memory for received tiles; the backend keeps its own larger cache. */
const TILE_BUDGET_BYTES = 384 * 1024 * 1024;
const SAVE_STATE_DELAY_MS = 800;
const ZOOM_STEP = 1.25;

export interface ViewerApi {
  zoomIn(): void;
  zoomOut(): void;
  setZoom(zoom: number): void;
  setFit(mode: FitMode): void;
  goToPage(page: number): void;
  /** Whether the document fits the window width (no horizontal scrolling). */
  fitsHorizontally(): boolean;
}

export interface ViewerStatus {
  page: number;
  zoom: number;
  fit: FitMode;
}

interface Props {
  doc: OpenedDocument;
  /** Registers the handler for this document's tile channel. */
  subscribe: (handler: (msg: ChannelMessage) => void) => void;
  onReady?: (api: ViewerApi) => void;
  onStatus?: (status: ViewerStatus) => void;
}

function cssPixels(name: string, fallback: number): number {
  const v = Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
  return Number.isFinite(v) ? v : fallback;
}

export function Viewer(props: Props) {
  let scroller!: HTMLDivElement;
  const doc = props.doc;
  const pages = doc.info.pages;
  const gap = cssPixels("--viewer-page-gap", 16);
  const store = new TileStore(TILE_BUDGET_BYTES);
  const mounted = new Map<number, HTMLDivElement>();

  const [container, setContainer] = createSignal({ width: 0, height: 0 });
  const [zoom, setZoomSignal] = createSignal(doc.viewState?.zoom ?? 1);
  const [fitMode, setFitSignal] = createSignal<FitMode>(doc.viewState?.fit ?? "width");
  const [dpr, setDpr] = createSignal(window.devicePixelRatio || 1);
  const [range, setRange] = createSignal<[number, number]>([0, Math.min(2, pages.length - 1)]);
  const [failed, setFailed] = createSignal<ReadonlyMap<number, string>>(new Map());

  const layout = createMemo(() => computeLayout(pages, zoom(), gap, container().width));
  const level = createMemo(() => levelFor(zoom(), dpr()));
  const indices = createMemo(() => {
    const [a, b] = range();
    return Array.from({ length: Math.max(0, b - a + 1) }, (_, i) => a + i);
  });

  let frame = 0;
  let lastY = 0;
  let direction = 1;
  let restored = false;

  const view = (): ViewRect => ({
    x: scroller.scrollLeft,
    y: scroller.scrollTop,
    width: scroller.clientWidth,
    height: scroller.clientHeight,
  });

  // --- Talking to the backend: at most one set_viewport in flight. ---------
  let sending = false;
  let queued: Viewport | null = null;
  async function sendViewport(v: Viewport) {
    if (sending) {
      queued = { ...v, evicted: [...(queued?.evicted ?? []), ...v.evicted] };
      return;
    }
    sending = true;
    await commands.setViewport(doc.id, v);
    sending = false;
    const next = queued;
    queued = null;
    if (next) void sendViewport(next);
  }

  // --- Tiles on pages. -------------------------------------------------------
  /** Whether the current level fully covers the visible part of page `p`. */
  function covered(p: number, lv: number, v: ViewRect): boolean {
    const [region] = regionsIn(layout(), v, lv).filter((r) => r.page === p);
    if (!region) return true;
    const s = levelScale(lv);
    const x0 = Math.floor((region.rect.x0 * s) / TILE_SIZE);
    const x1 = Math.floor((region.rect.x1 * s - 1e-3) / TILE_SIZE);
    const y0 = Math.floor((region.rect.y0 * s) / TILE_SIZE);
    const y1 = Math.floor((region.rect.y1 * s - 1e-3) / TILE_SIZE);
    for (let y = y0; y <= y1; y++) {
      for (let x = x0; x <= x1; x++) {
        if (!store.get(tileKey(p, lv, x, y))) return false;
      }
    }
    return true;
  }

  /**
   * Shows the sharpest tiles we have for page `p`: current-level tiles always,
   * other levels only as a fallback until the current level covers the view.
   */
  function syncPage(p: number, lv: number, v: ViewRect) {
    const el = mounted.get(p);
    if (!el) return;
    const complete = covered(p, lv, v);
    const now = performance.now();
    for (const tile of store.forPage(p)) {
      const show = tile.level === lv || !complete;
      if (show) {
        tile.lastUsed = now;
        if (tile.canvas.parentElement !== el) el.appendChild(tile.canvas);
      } else if (tile.canvas.parentElement) {
        tile.canvas.remove();
      }
    }
  }

  async function onMessage(msg: ChannelMessage) {
    if (msg.kind === "pageFailed") {
      setFailed((m) => new Map(m).set(msg.page, msg.message));
      return;
    }
    const size = pages[msg.page];
    if (!size) return;
    const tile = await createTile(msg, size);
    store.add(tile);
    if (mounted.has(msg.page)) syncPage(msg.page, untrack(level), view());
  }

  // --- The per-frame update. ---------------------------------------------------
  function schedule() {
    if (!frame) frame = requestAnimationFrame(update);
  }

  function update() {
    frame = 0;
    const l = layout();
    const v = view();
    if (v.width === 0 || pages.length === 0) return;
    if (v.y !== lastY) direction = v.y > lastY ? 1 : -1;
    lastY = v.y;

    const [first, last] = pagesBetween(l, v.y, v.y + v.height);
    const lo = Math.max(0, first - 1);
    const hi = Math.min(pages.length - 1, last + 1);
    const [a, b] = untrack(range);
    if (a !== lo || b !== hi) setRange([lo, hi]);

    const lv = level();
    const blank = new Set<number>();
    for (let p = first; p <= last; p++) {
      if (!store.hasPage(p)) blank.add(p);
      syncPage(p, lv, v);
    }
    store.evict((tile) => tile.level === lv && tile.page >= lo && tile.page <= hi);
    const { visible, nearby } = viewportRegions(l, v, lv, direction, blank);
    void sendViewport({ visible, nearby, evicted: store.takeEvicted() });

    props.onStatus?.({ page: pageAt(l, v.y + v.height / 3), zoom: zoom(), fit: fitMode() });
    saveStateSoon();
  }

  // --- Zoom. -------------------------------------------------------------------
  /** Zooms keeping the content point under `anchor` (scroller coordinates) fixed. */
  function applyZoom(next: number, anchor: { x: number; y: number } | null) {
    const target = clampZoom(next);
    if (Math.abs(target - untrack(zoom)) < 1e-6) return;
    const old = untrack(layout);
    const ax = anchor?.x ?? scroller.clientWidth / 2;
    const ay = anchor?.y ?? scroller.clientHeight / 2;
    const cx = scroller.scrollLeft + ax;
    const cy = scroller.scrollTop + ay;
    const p = pageAt(old, cy);
    const lx = (cx - (old.lefts[p] ?? 0)) / old.scale;
    const ly = (cy - (old.tops[p] ?? 0)) / old.scale;
    setZoomSignal(target); // Solid updates the DOM synchronously.
    const nl = untrack(layout);
    scroller.scrollLeft = (nl.lefts[p] ?? 0) + lx * nl.scale - ax;
    scroller.scrollTop = (nl.tops[p] ?? 0) + ly * nl.scale - ay;
    schedule();
  }

  function currentPage(): number {
    return pageAt(untrack(layout), scroller.scrollTop + scroller.clientHeight / 3);
  }

  // Fit modes follow the window size and the current page.
  createEffect(() => {
    const mode = fitMode();
    const c = container();
    if (c.width === 0) return;
    const page = pages[untrack(currentPage)];
    const z = page ? fitZoom(mode, page, c, gap) : null;
    if (z !== null) applyZoom(z, { x: c.width / 2, y: 0 });
  });

  const api: ViewerApi = {
    zoomIn: () => {
      setFitSignal("free");
      applyZoom(untrack(zoom) * ZOOM_STEP, null);
    },
    zoomOut: () => {
      setFitSignal("free");
      applyZoom(untrack(zoom) / ZOOM_STEP, null);
    },
    setZoom: (z) => {
      setFitSignal("free");
      applyZoom(z, null);
    },
    setFit: (mode) => {
      setFitSignal(mode);
      schedule();
    },
    goToPage: (p) => {
      const l = untrack(layout);
      const i = Math.min(pages.length - 1, Math.max(0, p));
      scroller.scrollTop = (l.tops[i] ?? 0) - gap;
    },
    fitsHorizontally: () => scroller.scrollWidth <= scroller.clientWidth + 1,
  };

  // --- Remembering the position. -----------------------------------------------
  let saveTimer = 0;
  function saveStateSoon() {
    if (!restored) return;
    clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      const l = untrack(layout);
      const y = scroller.scrollTop;
      const p = pageAt(l, y + gap);
      const h = l.heights[p] || 1;
      const pageOffset = Math.min(1, Math.max(0, (y - (l.tops[p] ?? 0)) / h));
      const maxX = scroller.scrollWidth - scroller.clientWidth;
      const scrollX = maxX > 0 ? scroller.scrollLeft / maxX : 0;
      void commands.saveViewState(doc.id, {
        page: p,
        pageOffset,
        scrollX,
        zoom: untrack(zoom),
        fit: untrack(fitMode),
      });
    }, SAVE_STATE_DELAY_MS);
  }

  function restore() {
    const state = doc.viewState;
    if (state) {
      const l = untrack(layout);
      const p = Math.min(pages.length - 1, state.page);
      scroller.scrollTop = (l.tops[p] ?? 0) + state.pageOffset * (l.heights[p] ?? 0);
      const maxX = scroller.scrollWidth - scroller.clientWidth;
      scroller.scrollLeft = state.scrollX * Math.max(0, maxX);
    }
    restored = true;
  }

  onMount(() => {
    props.subscribe((msg) => void onMessage(msg));

    const resize = new ResizeObserver(() => {
      setContainer({ width: scroller.clientWidth, height: scroller.clientHeight });
      schedule();
    });
    resize.observe(scroller);
    setContainer({ width: scroller.clientWidth, height: scroller.clientHeight });
    restore();

    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault(); // also blocks the webview's own page zoom
      const r = scroller.getBoundingClientRect();
      const lines = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? 33 : 1;
      const factor = Math.min(2, Math.max(0.5, Math.exp(-e.deltaY * lines * 0.0025)));
      setFitSignal("free");
      applyZoom(untrack(zoom) * factor, { x: e.clientX - r.left, y: e.clientY - r.top });
    };
    scroller.addEventListener("wheel", onWheel, { passive: false });

    // Moving the window to a screen with another pixel ratio changes the level.
    let media: MediaQueryList | null = null;
    const watchDpr = () => {
      media?.removeEventListener("change", onDpr);
      media = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
      media.addEventListener("change", onDpr);
    };
    const onDpr = () => {
      setDpr(window.devicePixelRatio || 1);
      watchDpr();
      schedule();
    };
    watchDpr();

    props.onReady?.(api);
    // Keyboard scrolling works at once; the ring only shows for keyboard users.
    scroller.focus({ preventScroll: true, focusVisible: false } as FocusOptions);
    schedule();

    onCleanup(() => {
      resize.disconnect();
      scroller.removeEventListener("wheel", onWheel);
      media?.removeEventListener("change", onDpr);
      cancelAnimationFrame(frame);
      clearTimeout(saveTimer);
      store.clear();
    });
  });

  return (
    <div
      class="viewer-scroll"
      ref={scroller}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: scrollable regions must be keyboard-focusable (WCAG 2.1.1) so arrows and PageUp/PageDown scroll the document.
      tabIndex={0}
      onScroll={schedule}
      role="document"
      aria-label={doc.info.metadata.title ?? doc.fileName}
    >
      <div
        class="viewer-content"
        style={{ width: `${layout().contentWidth}px`, height: `${layout().contentHeight}px` }}
      >
        <For each={indices()}>
          {(p) => {
            onCleanup(() => mounted.delete(p));
            return (
              <div
                class="page"
                ref={(el) => {
                  mounted.set(p, el);
                  queueMicrotask(() => syncPage(p, untrack(level), view()));
                }}
                role="img"
                aria-label={t("viewer.page", { page: String(p + 1), count: String(pages.length) })}
                style={{
                  transform: `translate(${layout().lefts[p]}px, ${layout().tops[p]}px)`,
                  width: `${Math.round(layout().widths[p] ?? 0)}px`,
                  height: `${Math.round(layout().heights[p] ?? 0)}px`,
                }}
              >
                <Show when={failed().get(p)}>
                  {(message) => (
                    <div class="page-error" title={message()}>
                      {t("viewer.pageFailed")}
                    </div>
                  )}
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
}
