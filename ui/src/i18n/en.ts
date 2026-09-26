// SPDX-License-Identifier: AGPL-3.0-or-later

/** English strings. This file defines the set of keys every locale must provide. */
export const en = {
  "app.title": "OpenPDFast",
  "viewer.empty.title": "No document open",
  "viewer.empty.hint": "Open a PDF with Ctrl+O or drop it here.",
  "viewer.drop": "Drop the PDF to open it",
  "viewer.page": "Page {page} of {count}",
  "viewer.pageFailed": "This page could not be rendered",
  "viewer.opening": "Opening {name}…",
  "toolbar.label": "Document tools",
  "toolbar.open": "Open",
  "toolbar.zoomIn": "Zoom in",
  "toolbar.zoomOut": "Zoom out",
  "toolbar.zoom": "Zoom level",
  "toolbar.fitWidth": "Fit width",
  "toolbar.fitPage": "Fit page",
  "toolbar.actualSize": "Actual size",
  "toolbar.pageOf": "{page} / {count}",
  "password.title": "Password required",
  "password.label": "“{name}” is protected. Enter its password:",
  "password.wrong": "Wrong password. Try again.",
  "password.submit": "Open",
  "password.cancel": "Cancel",
  "error.notFound": "The file does not exist or cannot be read.",
  "error.empty": "The document has no pages.",
  "error.invalid": "This file is not a supported document or it is damaged.",
  "error.generic": "The document could not be opened.",
  "error.dismiss": "Dismiss",
  "dialog.pdfFilter": "PDF documents",
  "about.version": "Version {version}",
  "about.license": "License: {license}",
  "about.source": "Source code",
} as const;

export type MessageKey = keyof typeof en;
