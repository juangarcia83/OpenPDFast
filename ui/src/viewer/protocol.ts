// SPDX-License-Identifier: AGPL-3.0-or-later
// Decoder for the raw tile channel (layout documented in crates/app/src/documents.rs).

const MSG_TILE = 1;
const MSG_PAGE_FAILED = 2;
const TILE_HEADER = 28;

export interface TileMessage {
  kind: "tile";
  page: number;
  level: number;
  x: number;
  y: number;
  width: number;
  height: number;
  /** A view into the received buffer; no copy. */
  pixels: Uint8ClampedArray<ArrayBuffer>;
}

export interface PageFailedMessage {
  kind: "pageFailed";
  page: number;
  message: string;
}

export type ChannelMessage = TileMessage | PageFailedMessage;

export function parseMessage(buffer: ArrayBuffer): ChannelMessage | null {
  if (buffer.byteLength < 8) return null;
  const view = new DataView(buffer);
  const kind = view.getUint32(0, true);
  const page = view.getUint32(4, true);
  if (kind === MSG_PAGE_FAILED) {
    return {
      kind: "pageFailed",
      page,
      message: new TextDecoder().decode(new Uint8Array(buffer, 8)),
    };
  }
  if (kind !== MSG_TILE || buffer.byteLength < TILE_HEADER) return null;
  const width = view.getUint32(20, true);
  const height = view.getUint32(24, true);
  const size = width * height * 4;
  if (size === 0 || buffer.byteLength !== TILE_HEADER + size) return null;
  return {
    kind: "tile",
    page,
    level: view.getInt32(8, true),
    x: view.getUint32(12, true),
    y: view.getUint32(16, true),
    width,
    height,
    pixels: new Uint8ClampedArray(buffer, TILE_HEADER, size),
  };
}
