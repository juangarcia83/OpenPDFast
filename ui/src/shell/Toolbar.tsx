// SPDX-License-Identifier: AGPL-3.0-or-later
import { Show } from "solid-js";
import { t } from "../i18n";
import type { ViewerApi, ViewerStatus } from "../viewer/Viewer";

interface Props {
  api: ViewerApi | null;
  status: ViewerStatus | null;
  pageCount: number;
  onOpen: () => void;
}

export function Toolbar(props: Props) {
  const percent = () => `${Math.round((props.status?.zoom ?? 1) * 100)} %`;
  return (
    <div class="toolbar" role="toolbar" aria-label={t("toolbar.label")}>
      <button type="button" class="btn" onClick={() => props.onOpen()} title="Ctrl+O">
        {t("toolbar.open")}
      </button>
      <Show when={props.api}>
        {(api) => (
          <>
            <span class="toolbar-sep" />
            <button
              type="button"
              class="btn btn-icon"
              onClick={() => api().zoomOut()}
              title={`${t("toolbar.zoomOut")} (Ctrl+-)`}
              aria-label={t("toolbar.zoomOut")}
            >
              −
            </button>
            <output class="toolbar-zoom" aria-label={t("toolbar.zoom")}>
              {percent()}
            </output>
            <button
              type="button"
              class="btn btn-icon"
              onClick={() => api().zoomIn()}
              title={`${t("toolbar.zoomIn")} (Ctrl++)`}
              aria-label={t("toolbar.zoomIn")}
            >
              +
            </button>
            <span class="toolbar-sep" />
            <button
              type="button"
              class="btn"
              aria-pressed={props.status?.fit === "width"}
              onClick={() => api().setFit("width")}
              title="Ctrl+1"
            >
              {t("toolbar.fitWidth")}
            </button>
            <button
              type="button"
              class="btn"
              aria-pressed={props.status?.fit === "page"}
              onClick={() => api().setFit("page")}
              title="Ctrl+2"
            >
              {t("toolbar.fitPage")}
            </button>
            <button type="button" class="btn" onClick={() => api().setZoom(1)} title="Ctrl+0">
              {t("toolbar.actualSize")}
            </button>
            <span class="toolbar-page" aria-live="polite">
              {t("toolbar.pageOf", {
                page: String((props.status?.page ?? 0) + 1),
                count: String(props.pageCount),
              })}
            </span>
          </>
        )}
      </Show>
    </div>
  );
}
