// SPDX-License-Identifier: AGPL-3.0-or-later
import { openUrl } from "@tauri-apps/plugin-opener";
import { createResource, For, Show } from "solid-js";
import { commands } from "../bindings";
import { IconExternal } from "../design/icons";
import { type Locale, locale, locales, t } from "../i18n";
import { chooseLocale } from "../settings";
import { Dialog } from "./Dialog";

const LANGUAGE_NAMES: Record<Locale, string> = { en: "English", es: "Español" };

export function AboutDialog(props: { onClose: () => void }) {
  const [info] = createResource(() => commands.appInfo());
  return (
    <Dialog title={t("about.title")} onClose={props.onClose}>
      <div class="dialog-body about">
        <img class="about-logo" src="/app-icon.svg" alt="" width="64" height="64" />
        <p class="about-tagline">{t("app.tagline")}</p>
        <Show when={info()}>
          {(app) => (
            <>
              <p>{t("about.version", { version: app().version })}</p>
              <p>{t("about.license", { license: app().license })}</p>
              {/* Visible link to the source code, required by the AGPL. */}
              <a
                class="link"
                href={app().sourceUrl}
                onClick={(event) => {
                  event.preventDefault();
                  void openUrl(app().sourceUrl);
                }}
              >
                {t("about.source")} <IconExternal size={14} />
              </a>
            </>
          )}
        </Show>
        <p class="dialog-note">{t("about.mupdf")}</p>
        <label class="field">
          <span>{t("about.language")}</span>
          <select
            class="input"
            value={locale()}
            onChange={(e) => chooseLocale(e.currentTarget.value as Locale)}
          >
            <For each={Object.keys(locales) as Locale[]}>
              {(l) => <option value={l}>{LANGUAGE_NAMES[l]}</option>}
            </For>
          </select>
        </label>
      </div>
    </Dialog>
  );
}
