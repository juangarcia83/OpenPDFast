// SPDX-License-Identifier: AGPL-3.0-or-later
import { openUrl } from "@tauri-apps/plugin-opener";
import { createEffect, createResource, Show } from "solid-js";
import "./App.css";
import { commands } from "./bindings";
import { locale, t } from "./i18n";

export function App() {
  const [info] = createResource(() => commands.appInfo());

  createEffect(() => {
    document.documentElement.lang = locale();
    document.title = t("app.title");
  });

  return (
    <div class="app">
      <main class="viewer" aria-label={t("app.title")}>
        <div class="empty">
          <h1 class="empty-title">{t("viewer.empty.title")}</h1>
          <p class="empty-hint">{t("viewer.empty.hint")}</p>
        </div>
      </main>
      <Show when={info()}>
        {(app) => (
          <footer class="status-bar">
            <span>{t("about.version", { version: app().version })}</span>
            <span>{t("about.license", { license: app().license })}</span>
            {/* Visible link to the source code, required by the AGPL. */}
            <a
              href={app().sourceUrl}
              onClick={(event) => {
                event.preventDefault();
                void openUrl(app().sourceUrl);
              }}
            >
              {t("about.source")}
            </a>
          </footer>
        )}
      </Show>
    </div>
  );
}
