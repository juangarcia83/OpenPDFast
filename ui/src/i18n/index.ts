// SPDX-License-Identifier: AGPL-3.0-or-later
import { createSignal } from "solid-js";
import { en, type MessageKey } from "./en";
import { es } from "./es";

export const locales = { en, es } satisfies Record<string, Record<MessageKey, string>>;
export type Locale = keyof typeof locales;

/** Picks the best supported locale from a list of BCP 47 tags, falling back to English. */
export function detectLocale(preferred: readonly string[]): Locale {
  for (const tag of preferred) {
    const lang = tag.toLowerCase().split("-")[0];
    if (lang && lang in locales) return lang as Locale;
  }
  return "en";
}

/** Replaces `{name}` placeholders with the given values; unknown ones are kept as-is. */
export function format(template: string, params: Record<string, string> = {}): string {
  return template.replace(/\{(\w+)\}/g, (match, name: string) => params[name] ?? match);
}

const [locale, setLocale] = createSignal<Locale>(
  detectLocale(typeof navigator === "undefined" ? [] : navigator.languages),
);

export { locale, setLocale };

/** Translates a key in the current locale. Reactive when called inside a tracking scope. */
export function t(key: MessageKey, params?: Record<string, string>): string {
  return format(locales[locale()][key], params);
}
