// SPDX-License-Identifier: AGPL-3.0-or-later
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { createEffect, createSignal, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import "./App.css";
import { commands, type DocumentError, type LayerInfo } from "./bindings";
import { IconAlert, IconClose } from "./design/icons";
import { locale, type MessageKey, t } from "./i18n";
import { applySettings, layersPanelOpen, setLayersPanelOpen } from "./settings";
import { AboutDialog } from "./shell/AboutDialog";
import { LayersPanel } from "./shell/LayersPanel";
import { PasswordDialog } from "./shell/PasswordDialog";
import { ShortcutsDialog } from "./shell/ShortcutsDialog";
import { StartScreen } from "./shell/StartScreen";
import { Toolbar } from "./shell/Toolbar";
import { closeDocument, type DocumentSession, openDocument } from "./viewer/session";
import { Viewer, type ViewerApi, type ViewerStatus } from "./viewer/Viewer";

const errorMessage: Partial<Record<DocumentError["kind"], MessageKey>> = {
  notFound: "error.notFound",
  empty: "error.empty",
  invalid: "error.invalid",
};

const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

type Modal = "shortcuts" | "about" | null;

export function App() {
  applySettings();

  const [session, setSession] = createSignal<DocumentSession | null>(null);
  const [api, setApi] = createSignal<ViewerApi | null>(null);
  const [status, setStatus] = createSignal<ViewerStatus | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [opening, setOpening] = createSignal<string | null>(null);
  const [password, setPassword] = createSignal<{ path: string; wrong: boolean } | null>(null);
  const [dragging, setDragging] = createSignal(false);
  const [modal, setModal] = createSignal<Modal>(null);
  const [layers, setLayers] = createSignal<LayerInfo[]>([]);
  const [layersBusy, setLayersBusy] = createSignal(false);
  let pageInput: HTMLInputElement | undefined;

  async function openPath(path: string, pass: string | null = null) {
    setError(null);
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
    const previous = session();
    setApi(null);
    setStatus(null);
    setLayers(result.session.doc.layers);
    setSession(result.session);
    if (previous) closeDocument(previous);
  }

  async function toggleLayer(layer: LayerInfo) {
    const doc = session()?.doc;
    if (!doc) return;
    setLayersBusy(true);
    // Radio entries can only be switched on; the group switches the rest off.
    const visible = layer.kind === "radio" ? true : !layer.visible;
    const result = await commands.setLayer(doc.id, layer.index, visible);
    setLayersBusy(false);
    if (result.status === "ok") setLayers(result.data);
    else setError(t("error.generic"));
  }

  async function chooseFile() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t("dialog.pdfFilter"), extensions: ["pdf"] }],
    });
    if (typeof path === "string") await openPath(path);
  }

  function goHome() {
    const previous = session();
    setApi(null);
    setStatus(null);
    setSession(null);
    setLayers([]);
    if (previous) closeDocument(previous);
  }

  const title = () => {
    const doc = session()?.doc;
    return doc ? (doc.info.metadata.title ?? doc.fileName) : null;
  };

  createEffect(() => {
    document.documentElement.lang = locale();
    const docTitle = title();
    const full = docTitle ? `${docTitle} — ${t("app.title")}` : t("app.title");
    document.title = full;
    void getCurrentWindow().setTitle(full);
  });

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
      if (e.key === "F1" || (e.key === "?" && !typing)) {
        e.preventDefault();
        setModal("shortcuts");
        return;
      }
      if (e.key === "F11") {
        e.preventDefault();
        const w = getCurrentWindow();
        void w.isFullscreen().then((f) => w.setFullscreen(!f));
        return;
      }
      const viewer = api();
      if (!typing && !e.ctrlKey && !e.metaKey && !e.altKey && viewer && status()) {
        const page = status()?.page ?? 0;
        if (e.key === "ArrowRight" && viewer.fitsHorizontally()) {
          e.preventDefault();
          viewer.goToPage(page + 1);
          return;
        }
        if (e.key === "ArrowLeft" && viewer.fitsHorizontally()) {
          e.preventDefault();
          viewer.goToPage(page - 1);
          return;
        }
      }
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const actions: Record<string, (() => void) | undefined> = {
        o: () => void chooseFile(),
        g: () => pageInput?.focus(),
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
        const first = p.paths.find((f) => f.toLowerCase().endsWith(".pdf")) ?? p.paths[0];
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
        title={title()}
        onOpen={() => void chooseFile()}
        onHome={goHome}
        onShortcuts={() => setModal("shortcuts")}
        onAbout={() => setModal("about")}
        layersOpen={layers().length > 0 ? layersPanelOpen() : null}
        onToggleLayers={() => setLayersPanelOpen(!layersPanelOpen())}
        registerPageInput={(el) => {
          pageInput = el;
        }}
      />
      <div class="body">
        <main class="workspace" classList={{ dragging: dragging() }}>
          <Switch>
            <Match when={session()} keyed>
              {(s) => (
                <Viewer doc={s.doc} subscribe={s.subscribe} onReady={setApi} onStatus={setStatus} />
              )}
            </Match>
            <Match when={!session()}>
              <StartScreen
                onOpenDialog={() => void chooseFile()}
                onOpenPath={(p) => void openPath(p)}
              />
            </Match>
          </Switch>
          <Show when={dragging()}>
            <div class="drop-overlay" aria-hidden="true">
              {t("viewer.drop")}
            </div>
          </Show>
          <div class="toasts" aria-live="polite">
            <Show when={opening()}>
              {(name) => (
                <div class="toast" role="status">
                  <span class="spinner" aria-hidden="true" />
                  {t("viewer.opening", { name: name() })}
                </div>
              )}
            </Show>
            <Show when={error()}>
              {(message) => (
                <div class="toast toast-error" role="alert">
                  <IconAlert />
                  <span>{message()}</span>
                  <button
                    type="button"
                    class="btn btn-icon"
                    aria-label={t("error.dismiss")}
                    onClick={() => setError(null)}
                  >
                    <IconClose size={16} />
                  </button>
                </div>
              )}
            </Show>
          </div>
        </main>
        <Show when={layers().length > 0 && layersPanelOpen()}>
          <LayersPanel
            layers={layers()}
            busy={layersBusy()}
            onToggle={(l) => void toggleLayer(l)}
            onClose={() => setLayersPanelOpen(false)}
          />
        </Show>
      </div>
      <Switch>
        <Match when={modal() === "shortcuts"}>
          <ShortcutsDialog onClose={() => setModal(null)} />
        </Match>
        <Match when={modal() === "about"}>
          <AboutDialog onClose={() => setModal(null)} />
        </Match>
      </Switch>
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
