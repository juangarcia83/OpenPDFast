// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, expect, it } from "vitest";
import type { MessageKey } from "./en";
import { detectLocale, format, locales } from "./index";

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("i18n", () => {
  it("every locale defines every key with a non-empty string", () => {
    const keys = Object.keys(locales.en).sort();
    for (const messages of Object.values(locales)) {
      expect(Object.keys(messages).sort()).toEqual(keys);
      for (const value of Object.values(messages)) expect(value.trim()).not.toBe("");
    }
  });

  it("every locale uses the same placeholders as English", () => {
    for (const messages of Object.values(locales)) {
      for (const [key, value] of Object.entries(locales.en)) {
        expect(placeholders(messages[key as MessageKey])).toEqual(placeholders(value));
      }
    }
  });

  it("detects the locale from BCP 47 tags", () => {
    expect(detectLocale(["es-ES", "en"])).toBe("es");
    expect(detectLocale(["fr-FR", "en-US"])).toBe("en");
    expect(detectLocale([])).toBe("en");
  });

  it("formats placeholders and keeps unknown ones", () => {
    expect(format("v{version} {x}", { version: "1.0" })).toBe("v1.0 {x}");
  });
});
