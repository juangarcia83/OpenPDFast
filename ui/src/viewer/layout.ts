// SPDX-License-Identifier: AGPL-3.0-or-later
// Pure layout math for the continuous vertical viewer. No DOM access here.

import type { FitMode, PageRegion, PageSize } from "../bindings";

/** 100 % zoom shows 1 pt as 1/72 in on a 96 dpi CSS pixel grid. */
export const CSS_PX_PER_PT = 96 / 72;
export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 64;
/** Mirrors `render::MIN_LEVEL` / `MAX_LEVEL` and `TILE_SIZE`. */
export const MIN_LEVEL = -12;
export const MAX_LEVEL = 18;
export const TILE_SIZE = 512;
/** Prefetch regions are rendered this many levels (factor 4) below the sharp level. */
export const PREVIEW_LEVEL_DROP = 4;

export interface Layout {
  /** CSS pixels per point. */
  scale: number;
  gap: number;
  contentWidth: number;
  contentHeight: number;
  tops: Float64Array;
  lefts: Float64Array;
  widths: Float64Array;
  heights: Float64Array;
}

/** A rectangle in content (scrollable) CSS pixel coordinates. */
export interface ViewRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const clampZoom = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

export function computeLayout(
  pages: readonly PageSize[],
  zoom: number,
  gap: number,
  containerWidth: number,
): Layout {
  const n = pages.length;
  const scale = zoom * CSS_PX_PER_PT;
  const tops = new Float64Array(n);
  const lefts = new Float64Array(n);
  const widths = new Float64Array(n);
  const heights = new Float64Array(n);
  let y = gap;
  let maxWidth = 0;
  for (let i = 0; i < n; i++) {
    const page = pages[i] as PageSize;
    widths[i] = page.width * scale;
    heights[i] = page.height * scale;
    tops[i] = y;
    y += (heights[i] as number) + gap;
    maxWidth = Math.max(maxWidth, widths[i] as number);
  }
  const contentWidth = Math.max(containerWidth, maxWidth + 2 * gap);
  for (let i = 0; i < n; i++) {
    // Whole pixels keep tile edges crisp.
    lefts[i] = Math.round((contentWidth - (widths[i] as number)) / 2);
  }
  return { scale, gap, contentWidth, contentHeight: y, tops, lefts, widths, heights };
}

/** Index of the page at content offset `y` (the gap above a page belongs to it). */
export function pageAt(layout: Layout, y: number): number {
  const { tops, gap } = layout;
  let lo = 0;
  let hi = tops.length - 1;
  if (hi < 0) return 0;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if ((tops[mid] as number) - gap <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** First and last page intersecting the content band `[y0, y1]`. */
export function pagesBetween(layout: Layout, y0: number, y1: number): [number, number] {
  return [pageAt(layout, y0), pageAt(layout, y1)];
}

/** Smallest level whose resolution is at least what the screen shows (never upscale). */
export function levelFor(zoom: number, devicePixelRatio: number): number {
  const deviceScale = zoom * CSS_PX_PER_PT * devicePixelRatio;
  if (!Number.isFinite(deviceScale) || deviceScale <= 0) return 0;
  const level = Math.ceil(2 * Math.log2(deviceScale) - 1e-4);
  // `+ 0` turns -0 (from ceil of a tiny negative) into 0.
  return Math.min(MAX_LEVEL, Math.max(MIN_LEVEL, level)) + 0;
}

export const levelScale = (level: number) => 2 ** (level / 2);

/** Page size in device pixels at `level`, as the backend computes it. */
export function pagePixels(page: PageSize, level: number): [number, number] {
  const s = levelScale(level);
  return [Math.max(1, Math.ceil(page.width * s)), Math.max(1, Math.ceil(page.height * s))];
}

/** Parts of each page intersecting `view`, in page points, at `level`. */
export function regionsIn(layout: Layout, view: ViewRect, level: number): PageRegion[] {
  const out: PageRegion[] = [];
  if (layout.tops.length === 0 || view.width <= 0 || view.height <= 0) return out;
  const [first, last] = pagesBetween(layout, view.y, view.y + view.height);
  for (let p = first; p <= last; p++) {
    const left = layout.lefts[p] as number;
    const top = layout.tops[p] as number;
    const x0 = Math.max(view.x, left);
    const y0 = Math.max(view.y, top);
    const x1 = Math.min(view.x + view.width, left + (layout.widths[p] as number));
    const y1 = Math.min(view.y + view.height, top + (layout.heights[p] as number));
    if (x1 <= x0 || y1 <= y0) continue;
    const s = layout.scale;
    out.push({
      page: p,
      level,
      rect: { x0: (x0 - left) / s, y0: (y0 - top) / s, x1: (x1 - left) / s, y1: (y1 - top) / s },
    });
  }
  return out;
}

/**
 * What to ask the backend for: the visible area sharp, one screen ahead in the
 * scroll direction sharp, and a few more screens as cheap low-resolution
 * previews. `blankPages` get a preview of their visible part first, so a jump
 * shows something within a frame or two instead of white.
 */
export function viewportRegions(
  layout: Layout,
  view: ViewRect,
  level: number,
  direction: number,
  blankPages: ReadonlySet<number>,
): { visible: PageRegion[]; nearby: PageRegion[] } {
  const preview = Math.max(MIN_LEVEL, level - PREVIEW_LEVEL_DROP);
  const sharp = regionsIn(layout, view, level);
  const blank =
    preview < level ? regionsIn(layout, view, preview).filter((r) => blankPages.has(r.page)) : [];
  const h = view.height;
  const down = direction >= 0;
  const band = (before: number, after: number): ViewRect => ({
    x: view.x - view.width / 2,
    y: view.y - (down ? before : after) * h,
    width: view.width * 2,
    height: h * (1 + before + after),
  });
  const nearSharp = regionsIn(layout, band(0.25, 1), level);
  const farPreview = regionsIn(layout, band(1, 3), preview);
  return { visible: [...blank, ...sharp], nearby: [...nearSharp, ...farPreview] };
}

/** Zoom that fits `page` into the container according to `mode`. */
export function fitZoom(
  mode: FitMode,
  page: PageSize,
  container: { width: number; height: number },
  gap: number,
): number | null {
  if (mode === "free") return null;
  const byWidth = (container.width - 2 * gap) / (page.width * CSS_PX_PER_PT);
  if (mode === "width") return clampZoom(byWidth);
  const byHeight = (container.height - 2 * gap) / (page.height * CSS_PX_PER_PT);
  return clampZoom(Math.min(byWidth, byHeight));
}
