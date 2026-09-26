// SPDX-License-Identifier: AGPL-3.0-or-later
import { For, Match, Switch } from "solid-js";
import type { LayerInfo } from "../bindings";
import { IconClose } from "../design/icons";
import { t } from "../i18n";

interface Props {
  layers: LayerInfo[];
  busy: boolean;
  onToggle: (layer: LayerInfo) => void;
  onClose: () => void;
}

/** Optional content (OCG) of the document: toggle what the plan shows. */
export function LayersPanel(props: Props) {
  return (
    <aside class="side-panel" aria-labelledby="layers-title">
      <header class="side-panel-header">
        <h2 id="layers-title" class="section-title">
          {t("layers.title")}
        </h2>
        <button
          type="button"
          class="btn btn-icon"
          aria-label={t("layers.close")}
          title={t("layers.close")}
          onClick={() => props.onClose()}
        >
          <IconClose size={16} />
        </button>
      </header>
      <ul class="layer-list" aria-busy={props.busy}>
        <For each={props.layers}>
          {(layer) => (
            <li
              class="layer-item"
              style={{ "padding-left": `calc(${layer.depth} * var(--space-4))` }}
            >
              <Switch>
                <Match when={layer.kind === "label"}>
                  <span class="layer-label">{layer.name}</span>
                </Match>
                <Match when={layer.kind !== "label"}>
                  <label class="layer-toggle" classList={{ locked: layer.locked }}>
                    <input
                      type={layer.kind === "radio" ? "radio" : "checkbox"}
                      checked={layer.visible}
                      disabled={layer.locked || props.busy}
                      onChange={() => props.onToggle(layer)}
                    />
                    <span>{layer.name || t("layers.unnamed")}</span>
                  </label>
                </Match>
              </Switch>
            </li>
          )}
        </For>
      </ul>
    </aside>
  );
}
