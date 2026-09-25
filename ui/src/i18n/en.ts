// SPDX-License-Identifier: AGPL-3.0-or-later

/** English strings. This file defines the set of keys every locale must provide. */
export const en = {
  "app.title": "PDF Reader",
  "viewer.empty.title": "No document open",
  "viewer.empty.hint": "Opening PDFs arrives in the next milestone.",
  "about.version": "Version {version}",
  "about.license": "License: {license}",
  "about.source": "Source code",
} as const;

export type MessageKey = keyof typeof en;
