// SPDX-License-Identifier: AGPL-3.0-or-later
// Per-user interface preferences kept in localStorage (a convenience only:
// everything works with the defaults if storage is unavailable).

import { createEffect, createSignal } from "solid-js";
import { type Locale, locales, setLocale } from "./i18n";

export type Theme = "system" | "light" | "dark";
const THEMES: readonly Theme[] = ["system", "light", "dark"];

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Private mode or blocked storage: keep the in-memory value only.
  }
}

const storedTheme = read("theme");
const [theme, setThemeSignal] = createSignal<Theme>(
  THEMES.includes(storedTheme as Theme) ? (storedTheme as Theme) : "system",
);

export { theme };

export function setTheme(value: Theme) {
  setThemeSignal(value);
  write("theme", value);
}

/** system → light → dark → system. */
export function cycleTheme() {
  const i = THEMES.indexOf(theme());
  setTheme(THEMES[(i + 1) % THEMES.length] as Theme);
}

const [layersPanelOpen, setLayersPanelSignal] = createSignal(read("layersPanel") !== "closed");

export { layersPanelOpen };

/** Panels remember whether they were open (AGENTS.md §8). */
export function setLayersPanelOpen(open: boolean) {
  setLayersPanelSignal(open);
  write("layersPanel", open ? "open" : "closed");
}

export function chooseLocale(value: Locale) {
  setLocale(value);
  write("locale", value);
}

/** Applies stored preferences; call once at startup inside a reactive root. */
export function applySettings() {
  const storedLocale = read("locale");
  if (storedLocale && storedLocale in locales) setLocale(storedLocale as Locale);
  createEffect(() => {
    const t = theme();
    if (t === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = t;
  });
}
