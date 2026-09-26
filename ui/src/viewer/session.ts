// SPDX-License-Identifier: AGPL-3.0-or-later
// Opening a document: one tile channel per document, decoded on arrival.

import { Channel } from "@tauri-apps/api/core";
import { commands, type DocumentError, type OpenedDocument } from "../bindings";
import { type ChannelMessage, parseMessage } from "./protocol";

export interface DocumentSession {
  doc: OpenedDocument;
  subscribe: (handler: (msg: ChannelMessage) => void) => void;
}

export type OpenResult =
  | { ok: true; session: DocumentSession }
  | { ok: false; error: DocumentError };

export async function openDocument(path: string, password: string | null): Promise<OpenResult> {
  let handler: ((msg: ChannelMessage) => void) | null = null;
  // Messages can arrive before the viewer subscribes (exact page sizes are
  // measured right after opening); keep them until it does.
  const pending: ChannelMessage[] = [];
  const channel = new Channel<ArrayBuffer>((buffer) => {
    const msg = parseMessage(buffer);
    if (!msg) return;
    if (handler) handler(msg);
    else pending.push(msg);
  });
  const result = await commands.openDocument(path, password, channel);
  if (result.status === "error") return { ok: false, error: result.error };
  return {
    ok: true,
    session: {
      doc: result.data,
      subscribe: (h) => {
        handler = h;
        for (const msg of pending.splice(0)) h(msg);
      },
    },
  };
}

export function closeDocument(session: DocumentSession) {
  void commands.closeDocument(session.doc.id);
}
