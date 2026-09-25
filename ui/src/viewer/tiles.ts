// SPDX-License-Identifier: AGPL-3.0-or-later
// Received tiles, kept as GPU-backed canvases so re-showing a tile is free.

import type { PageSize, TileKey } from "../bindings";
import { pagePixels, TILE_SIZE } from "./layout";
import type { TileMessage } from "./protocol";

export interface Tile {
  key: string;
  page: number;
  level: number;
  x: number;
  y: number;
  canvas: HTMLCanvasElement;
  bytes: number;
  lastUsed: number;
}

export const tileKey = (page: number, level: number, x: number, y: number) =>
  `${page}:${level}:${x}:${y}`;

/** Uploads a decoded tile to a canvas without copying it on the CPU side. */
export async function createTile(msg: TileMessage, pageSize: PageSize): Promise<Tile> {
  const bitmap = await createImageBitmap(new ImageData(msg.pixels, msg.width, msg.height));
  const canvas = document.createElement("canvas");
  canvas.width = msg.width;
  canvas.height = msg.height;
  const ctx = canvas.getContext("bitmaprenderer");
  if (ctx) ctx.transferFromImageBitmap(bitmap);
  else canvas.getContext("2d")?.drawImage(bitmap, 0, 0);
  // Position as a fraction of the page, so zooming rescales tiles instantly.
  const [pw, ph] = pagePixels(pageSize, msg.level);
  const pct = (v: number, total: number) => `${(v / total) * 100}%`;
  canvas.className = "tile";
  canvas.style.left = pct(msg.x * TILE_SIZE, pw);
  canvas.style.top = pct(msg.y * TILE_SIZE, ph);
  canvas.style.width = pct(msg.width, pw);
  canvas.style.height = pct(msg.height, ph);
  canvas.style.zIndex = String(100 + msg.level);
  canvas.setAttribute("aria-hidden", "true");
  return {
    key: tileKey(msg.page, msg.level, msg.x, msg.y),
    page: msg.page,
    level: msg.level,
    x: msg.x,
    y: msg.y,
    canvas,
    bytes: msg.width * msg.height * 4,
    lastUsed: performance.now(),
  };
}

export class TileStore {
  readonly #tiles = new Map<string, Tile>();
  readonly #byPage = new Map<number, Set<Tile>>();
  #bytes = 0;
  #evicted: TileKey[] = [];

  constructor(private readonly budget: number) {}

  get(key: string): Tile | undefined {
    return this.#tiles.get(key);
  }

  add(tile: Tile) {
    const old = this.#tiles.get(tile.key);
    if (old) this.#remove(old, false);
    this.#tiles.set(tile.key, tile);
    let set = this.#byPage.get(tile.page);
    if (!set) {
      set = new Set();
      this.#byPage.set(tile.page, set);
    }
    set.add(tile);
    this.#bytes += tile.bytes;
  }

  forPage(page: number): Iterable<Tile> {
    return this.#byPage.get(page) ?? [];
  }

  hasPage(page: number): boolean {
    return (this.#byPage.get(page)?.size ?? 0) > 0;
  }

  /** Drops least-recently-used tiles over budget, never those `keep` accepts. */
  evict(keep: (tile: Tile) => boolean) {
    if (this.#bytes <= this.budget) return;
    const candidates = [...this.#tiles.values()]
      .filter((t) => !keep(t))
      .sort((a, b) => a.lastUsed - b.lastUsed);
    for (const tile of candidates) {
      if (this.#bytes <= this.budget) break;
      this.#remove(tile, true);
    }
  }

  /** Tiles dropped since the last call, to report to the backend. */
  takeEvicted(): TileKey[] {
    const out = this.#evicted;
    this.#evicted = [];
    return out;
  }

  clear() {
    for (const tile of this.#tiles.values()) tile.canvas.remove();
    this.#tiles.clear();
    this.#byPage.clear();
    this.#bytes = 0;
    this.#evicted = [];
  }

  #remove(tile: Tile, report: boolean) {
    tile.canvas.remove();
    // Release GPU memory now instead of waiting for GC.
    tile.canvas.width = 0;
    tile.canvas.height = 0;
    this.#tiles.delete(tile.key);
    this.#byPage.get(tile.page)?.delete(tile);
    this.#bytes -= tile.bytes;
    if (report) this.#evicted.push({ page: tile.page, level: tile.level, x: tile.x, y: tile.y });
  }
}
