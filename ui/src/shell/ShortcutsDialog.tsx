// SPDX-License-Identifier: AGPL-3.0-or-later
import { For } from "solid-js";
import { type MessageKey, t } from "../i18n";
import { Dialog } from "./Dialog";

const SHORTCUTS: [MessageKey, string[][]][] = [
  ["shortcuts.open", [["Ctrl", "O"]]],
  ["shortcuts.zoomIn", [["Ctrl", "+"]]],
  ["shortcuts.zoomOut", [["Ctrl", "−"]]],
  ["shortcuts.zoomWheel", [["Ctrl", "wheel"]]],
  ["shortcuts.actualSize", [["Ctrl", "0"]]],
  ["shortcuts.fitWidth", [["Ctrl", "1"]]],
  ["shortcuts.fitPage", [["Ctrl", "2"]]],
  ["shortcuts.nextPage", [["→"], ["←"], ["PgDn"], ["PgUp"]]],
  ["shortcuts.firstLast", [["Home"], ["End"]]],
  ["shortcuts.goTo", [["Ctrl", "G"]]],
  ["shortcuts.fullscreen", [["F11"]]],
  ["shortcuts.help", [["F1"], ["?"]]],
];

export function ShortcutsDialog(props: { onClose: () => void }) {
  return (
    <Dialog title={t("shortcuts.title")} onClose={props.onClose}>
      <dl class="dialog-body shortcuts">
        <For each={SHORTCUTS}>
          {([label, combos]) => (
            <>
              <dt>{t(label)}</dt>
              <dd>
                <For each={combos}>
                  {(keys) => (
                    <span class="combo">
                      <For each={keys}>
                        {(k) => <kbd>{k === "wheel" ? t("shortcuts.wheel") : k}</kbd>}
                      </For>
                    </span>
                  )}
                </For>
              </dd>
            </>
          )}
        </For>
      </dl>
    </Dialog>
  );
}
