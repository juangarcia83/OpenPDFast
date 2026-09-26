// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from "vitest";
import {
  CSS_PX_PER_PT,
  computeLayout,
  fitZoom,
  levelFor,
  pageAt,
  pagePixels,
  regionsIn,
  viewportRegions,
} from "./layout";
import { parseMessage } from "./protocol";

const letter = { width: 612, height: 792 };
const pages = [letter, { width: 792, height: 612 }, letter];

describe("layout", () => {
  const layout = computeLayout(pages, 1, 10, 800);

  it("stacks pages with gaps and centers them", () => {
    const s = CSS_PX_PER_PT;
    expect(layout.tops[0]).toBe(10);
    expect(layout.tops[1]).toBeCloseTo(10 + 792 * s + 10);
    expect(layout.contentHeight).toBeCloseTo(10 + (792 + 612 + 792) * s + 30);
    expect(layout.contentWidth).toBeCloseTo(Math.max(800, 792 * s + 20));
    expect(layout.lefts[0]).toBe(Math.round((layout.contentWidth - 612 * s) / 2));
  });

  it("finds the page at an offset", () => {
    expect(pageAt(layout, 0)).toBe(0);
    expect(pageAt(layout, (layout.tops[1] as number) + 1)).toBe(1);
    expect(pageAt(layout, (layout.tops[1] as number) - 5)).toBe(1); // gap above page 1
    expect(pageAt(layout, 1e9)).toBe(2);
  });

  it("maps a view rectangle to page regions in points", () => {
    const top = layout.tops[0] as number;
    const left = layout.lefts[0] as number;
    const s = layout.scale;
    const [r] = regionsIn(layout, { x: left, y: top, width: 100 * s, height: 50 * s }, 3);
    expect(r?.page).toBe(0);
    expect(r?.level).toBe(3);
    expect(r?.rect.x0).toBeCloseTo(0);
    expect(r?.rect.x1).toBeCloseTo(100);
    expect(r?.rect.y1).toBeCloseTo(50);
  });

  it("asks for previews of blank pages first and prefetches ahead", () => {
    const view = { x: 0, y: 0, width: 800, height: 600 };
    const { visible, nearby } = viewportRegions(layout, view, 2, 1, new Set([0]));
    expect(visible[0]?.level).toBe(-2);
    expect(visible.some((r) => r.level === 2)).toBe(true);
    expect(nearby.some((r) => r.page === 1)).toBe(true);
  });

  it("chooses levels that never upscale", () => {
    expect(levelFor(1 / CSS_PX_PER_PT, 1)).toBe(0);
    expect(levelFor(1, 1)).toBe(1); // 1.333 -> sqrt(2)
    expect(levelFor(1, 2)).toBe(3); // 2.667 -> 2.83
    expect(levelFor(1e6, 1)).toBe(18);
    expect(pagePixels(letter, 2)).toEqual([1224, 1584]);
  });

  it("fits width and page", () => {
    const c = { width: 1000, height: 500 };
    expect(fitZoom("width", letter, c, 0)).toBeCloseTo(1000 / (612 * CSS_PX_PER_PT));
    expect(fitZoom("page", letter, c, 0)).toBeCloseTo(500 / (792 * CSS_PX_PER_PT));
    expect(fitZoom("free", letter, c, 0)).toBeNull();
  });
});

describe("protocol", () => {
  it("decodes tiles without copying pixels", () => {
    const buf = new ArrayBuffer(28 + 2 * 1 * 4);
    const v = new DataView(buf);
    [1, 7].forEach((n, i) => {
      v.setUint32(i * 4, n, true);
    });
    v.setInt32(8, -3, true);
    [1, 2, 2, 1].forEach((n, i) => {
      v.setUint32(12 + i * 4, n, true);
    });
    const msg = parseMessage(buf);
    expect(msg).toMatchObject({
      kind: "tile",
      page: 7,
      level: -3,
      x: 1,
      y: 2,
      width: 2,
      height: 1,
    });
    if (msg?.kind === "tile") expect(msg.pixels.buffer).toBe(buf);
  });

  it("decodes page size updates", () => {
    const buf = new ArrayBuffer(12 + 16);
    const v = new DataView(buf);
    [3, 64, 2].forEach((n, i) => {
      v.setUint32(i * 4, n, true);
    });
    [612, 792, 842, 595].forEach((n, i) => {
      v.setFloat32(12 + i * 4, n, true);
    });
    expect(parseMessage(buf)).toEqual({
      kind: "pageSizes",
      first: 64,
      sizes: [
        { width: 612, height: 792 },
        { width: 842, height: 595 },
      ],
    });
  });

  it("decodes page failures and rejects garbage", () => {
    const text = new TextEncoder().encode("boom");
    const buf = new ArrayBuffer(8 + text.length);
    new DataView(buf).setUint32(0, 2, true);
    new DataView(buf).setUint32(4, 4, true);
    new Uint8Array(buf, 8).set(text);
    expect(parseMessage(buf)).toEqual({ kind: "pageFailed", page: 4, message: "boom" });
    expect(parseMessage(new ArrayBuffer(3))).toBeNull();
    expect(parseMessage(new ArrayBuffer(40))).toBeNull();
  });
});
