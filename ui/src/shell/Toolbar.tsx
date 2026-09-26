// SPDX-License-Identifier: AGPL-3.0-or-later
import { createEffect, createSignal, For, Show } from "solid-js";
import {
  IconFitPage,
  IconFitWidth,
  IconInfo,
  IconKeyboard,
  IconLayers,
  IconMinus,
  IconMoon,
  IconNext,
  IconOpen,
  IconPlus,
  IconPrev,
  IconSun,
  IconSystem,
} from "../design/icons";
import { t } from "../i18n";
import { cycleTheme, theme } from "../settings";
import type { ViewerApi, ViewerStatus } from "../viewer/Viewer";

const ZOOM_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

interface Props {
  api: ViewerApi | null;
  status: ViewerStatus | null;
  pageCount: number;
  title: string | null;
  onOpen: () => void;
  onHome: () => void;
  onShortcuts: () => void;
  onAbout: () => void;
  /** Present when the document has layers. */
  layersOpen: boolean | null;
  onToggleLayers: () => void;
  registerPageInput: (el: HTMLInputElement) => void;
}

export function Toolbar(props: Props) {
  const page = () => (props.status?.page ?? 0) + 1;
  const [draft, setDraft] = createSignal("1");
  let editing = false;
  createEffect(() => {
    const p = String(page());
    if (!editing) setDraft(p);
  });

  const commitPage = (input: HTMLInputElement) => {
    const n = Number.parseInt(draft(), 10);
    if (Number.isFinite(n) && props.api)
      props.api.goToPage(Math.min(props.pageCount, Math.max(1, n)) - 1);
    else setDraft(String(page()));
    input.blur();
  };

  const zoomLabel = () => `${Math.round((props.status?.zoom ?? 1) * 100)} %`;
  const themeIcon = () =>
    theme() === "light" ? <IconSun /> : theme() === "dark" ? <IconMoon /> : <IconSystem />;
  const themeLabel = () => t("toolbar.theme", { theme: t(`theme.${theme()}`) });

  return (
    <div class="toolbar" role="toolbar" aria-label={t("toolbar.label")}>
      <div class="toolbar-group">
        <button
          type="button"
          class="btn"
          onClick={() => props.onOpen()}
          title={`${t("toolbar.open")} (Ctrl+O)`}
        >
          <IconOpen /> <span class="btn-label">{t("toolbar.open")}</span>
        </button>
        <Show when={props.title}>
          {(title) => (
            <button
              type="button"
              class="toolbar-title"
              onClick={() => props.onHome()}
              title={t("toolbar.home")}
            >
              {title()}
            </button>
          )}
        </Show>
      </div>

      <Show when={props.api}>
        {(api) => (
          <>
            <div class="toolbar-group toolbar-center">
              <button
                type="button"
                class="btn btn-icon"
                onClick={() => api().goToPage(page() - 2)}
                disabled={page() <= 1}
                aria-label={t("toolbar.prevPage")}
                title={`${t("toolbar.prevPage")} (←)`}
              >
                <IconPrev />
              </button>
              <input
                ref={(el) => props.registerPageInput(el)}
                class="input page-input"
                inputmode="numeric"
                aria-label={t("toolbar.pageInput")}
                title={`${t("toolbar.pageInput")} (Ctrl+G)`}
                value={draft()}
                onFocus={(e) => {
                  editing = true;
                  e.currentTarget.select();
                }}
                onBlur={() => {
                  editing = false;
                  setDraft(String(page()));
                }}
                onInput={(e) => setDraft(e.currentTarget.value.replace(/\D/g, ""))}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitPage(e.currentTarget);
                  if (e.key === "Escape") e.currentTarget.blur();
                }}
              />
              <span class="toolbar-muted">
                {t("toolbar.of", { count: String(props.pageCount) })}
              </span>
              <button
                type="button"
                class="btn btn-icon"
                onClick={() => api().goToPage(page())}
                disabled={page() >= props.pageCount}
                aria-label={t("toolbar.nextPage")}
                title={`${t("toolbar.nextPage")} (→)`}
              >
                <IconNext />
              </button>
            </div>

            <div class="toolbar-group toolbar-zoom">
              <button
                type="button"
                class="btn btn-icon"
                onClick={() => api().zoomOut()}
                aria-label={t("toolbar.zoomOut")}
                title={`${t("toolbar.zoomOut")} (Ctrl+−)`}
              >
                <IconMinus />
              </button>
              <select
                class="input zoom-select"
                aria-label={t("toolbar.zoom")}
                value="current"
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  e.currentTarget.value = "current";
                  if (v === "width" || v === "page") api().setFit(v);
                  else api().setZoom(Number(v));
                }}
              >
                <option value="current" hidden>
                  {zoomLabel()}
                </option>
                <option value="width">{t("toolbar.fitWidth")}</option>
                <option value="page">{t("toolbar.fitPage")}</option>
                <For each={ZOOM_PRESETS}>
                  {(z) => <option value={z}>{Math.round(z * 100)} %</option>}
                </For>
              </select>
              <button
                type="button"
                class="btn btn-icon"
                onClick={() => api().zoomIn()}
                aria-label={t("toolbar.zoomIn")}
                title={`${t("toolbar.zoomIn")} (Ctrl++)`}
              >
                <IconPlus />
              </button>
              <button
                type="button"
                class="btn btn-icon"
                aria-pressed={props.status?.fit === "width"}
                onClick={() => api().setFit("width")}
                aria-label={t("toolbar.fitWidth")}
                title={`${t("toolbar.fitWidth")} (Ctrl+1)`}
              >
                <IconFitWidth />
              </button>
              <button
                type="button"
                class="btn btn-icon"
                aria-pressed={props.status?.fit === "page"}
                onClick={() => api().setFit("page")}
                aria-label={t("toolbar.fitPage")}
                title={`${t("toolbar.fitPage")} (Ctrl+2)`}
              >
                <IconFitPage />
              </button>
            </div>
          </>
        )}
      </Show>

      <div class="toolbar-group toolbar-end">
        <Show when={props.layersOpen !== null}>
          <button
            type="button"
            class="btn btn-icon"
            aria-pressed={props.layersOpen === true}
            onClick={() => props.onToggleLayers()}
            aria-label={t("toolbar.layers")}
            title={t("toolbar.layers")}
          >
            <IconLayers />
          </button>
        </Show>
        <button
          type="button"
          class="btn btn-icon"
          onClick={cycleTheme}
          aria-label={themeLabel()}
          title={themeLabel()}
        >
          {themeIcon()}
        </button>
        <button
          type="button"
          class="btn btn-icon"
          onClick={() => props.onShortcuts()}
          aria-label={t("toolbar.shortcuts")}
          title={`${t("toolbar.shortcuts")} (F1)`}
        >
          <IconKeyboard />
        </button>
        <button
          type="button"
          class="btn btn-icon"
          onClick={() => props.onAbout()}
          aria-label={t("toolbar.about")}
          title={t("toolbar.about")}
        >
          <IconInfo />
        </button>
      </div>
    </div>
  );
}
