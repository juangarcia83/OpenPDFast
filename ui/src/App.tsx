// SPDX-License-Identifier: AGPL-3.0-or-later
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  createEffect,
  createResource,
  createSignal,
  Match,
  onCleanup,
  onMount,
  Show,
  Switch,
} from "solid-js";
import "./App.css";
import { commands, type DocumentError } from "./bindings";
import { locale, type MessageKey, t } from "./i18n";
import { PasswordDialog } from "./shell/PasswordDialog";
import { Toolbar } from "./shell/Toolbar";
import { closeDocument, type DocumentSession, openDocument } from "./viewer/session";
import { Viewer, type ViewerApi, type ViewerStatus } from "./viewer/Viewer";

const errorMessage: Partial<Record<DocumentError["kind"], MessageKey>> = {
  notFound: "error.notFound",
  empty: "error.empty",
  invalid: "error.invalid",
};

const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

export function App() {
  const [info] = createResource(() => commands.appInfo());
  const [session, setSession] = createSignal<DocumentSession | null>(null);
  const [api, setApi] = createSignal<ViewerApi | null>(null);
  const [status, setStatus] = createSignal<ViewerStatus | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [opening, setOpening] = createSignal<string | null>(null);
  const [password, setPassword] = createSignal<{ path: string; wrong: boolean } | null>(null);
  const [dragging, setDragging] = createSignal(false);

  async function openPath(path: string, pass: string | null = null) {
    setOpening(baseName(path));
    const result = await openDocument(path, pass);
    setOpening(null);
    if (!result.ok) {
      const kind = result.error.kind;
      if (kind === "passwordRequired" || kind === "wrongPassword") {
        setPassword({ path, wrong: kind === "wrongPassword" });
      } else {
        setError(t(errorMessage[kind] ?? "error.generic"));
      }
      return;
    }
    setPassword(null);
    setError(null);
    const previous = session();
    setApi(null);
    setStatus(null);
    setSession(result.session);
    if (previous) closeDocument(previous);
  }

  async function chooseFile() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t("dialog.pdfFilter"), extensions: ["pdf"] }],
    });
    if (typeof path === "string") await openPath(path);
  }

  createEffect(() => {
    document.documentElement.lang = locale();
    const doc = session()?.doc;
    const title = doc
      ? `${doc.info.metadata.title ?? doc.fileName} — ${t("app.title")}`
      : t("app.title");
    document.title = title;
    void getCurrentWindow().setTitle(title);
  });

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const viewer = api();
      const actions: Record<string, (() => void) | undefined> = {
        o: () => void chooseFile(),
        "=": viewer?.zoomIn,
        "+": viewer?.zoomIn,
        "-": viewer?.zoomOut,
        "0": () => viewer?.setZoom(1),
        "1": () => viewer?.setFit("width"),
        "2": () => viewer?.setFit("page"),
      };
      const action = actions[e.key.toLowerCase()];
      if (action) {
        e.preventDefault();
        action();
      }
    };
    window.addEventListener("keydown", onKey);

    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") setDragging(true);
      else if (p.type === "leave") setDragging(false);
      else if (p.type === "drop") {
        setDragging(false);
        const [first] = p.paths;
        if (first) void openPath(first);
      }
    });

    void commands.startupFiles().then(([first]) => {
      if (first) void openPath(first);
    });

    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      void unlisten.then((f) => f());
    });
  });

  return (
    <div class="app">
      <Toolbar
        api={api()}
        status={status()}
        pageCount={session()?.doc.info.pages.length ?? 0}
        onOpen={() => void chooseFile()}
      />
      <main class="workspace" classList={{ dragging: dragging() }}>
        <Switch>
          <Match when={session()} keyed>
            {(s) => (
              <Viewer doc={s.doc} subscribe={s.subscribe} onReady={setApi} onStatus={setStatus} />
            )}
          </Match>
          <Match when={!session()}>
            <div class="empty">
              <h1 class="empty-title">{t("viewer.empty.title")}</h1>
              <p class="empty-hint">{t("viewer.empty.hint")}</p>
            </div>
          </Match>
        </Switch>
        <Show when={dragging()}>
          <div class="drop-overlay" aria-hidden="true">
            {t("viewer.drop")}
          </div>
        </Show>
        <Show when={opening()}>
          {(name) => (
            <div class="notice" role="status">
              {t("viewer.opening", { name: name() })}
            </div>
          )}
        </Show>
        <Show when={error()}>
          {(message) => (
            <div class="notice notice-error" role="alert">
              <span>{message()}</span>
              <button type="button" class="btn" onClick={() => setError(null)}>
                {t("error.dismiss")}
              </button>
            </div>
          )}
        </Show>
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
      <Show when={password()} keyed>
        {(p) => (
          <PasswordDialog
            fileName={baseName(p.path)}
            wrong={p.wrong}
            onSubmit={(value) => void openPath(p.path, value)}
            onCancel={() => setPassword(null)}
          />
        )}
      </Show>
    </div>
  );
}
