// SPDX-License-Identifier: AGPL-3.0-or-later
import { type JSX, onMount } from "solid-js";
import { IconClose } from "../design/icons";
import { t } from "../i18n";

interface Props {
  title: string;
  onClose: () => void;
  children: JSX.Element;
  /** Element to focus when the dialog opens (defaults to the close button). */
  initialFocus?: () => HTMLElement | undefined;
}

/** Modal dialog on top of the native <dialog>: focus trap, Esc and backdrop close. */
export function Dialog(props: Props) {
  let dialog!: HTMLDialogElement;
  let close!: HTMLButtonElement;
  const titleId = `dialog-${Math.random().toString(36).slice(2)}`;

  onMount(() => {
    dialog.showModal();
    (props.initialFocus?.() ?? close).focus();
  });

  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the click only detects the backdrop; the keyboard equivalent is Esc (onCancel).
    <dialog
      ref={dialog}
      class="dialog"
      aria-labelledby={titleId}
      onCancel={(e) => {
        e.preventDefault();
        props.onClose();
      }}
      onClick={(e) => {
        if (e.target === dialog) props.onClose(); // click on the backdrop
      }}
    >
      <header class="dialog-header">
        <h2 id={titleId} class="dialog-title">
          {props.title}
        </h2>
        <button
          ref={close}
          type="button"
          class="btn btn-icon"
          aria-label={t("shortcuts.close")}
          onClick={() => props.onClose()}
        >
          <IconClose />
        </button>
      </header>
      {props.children}
    </dialog>
  );
}
