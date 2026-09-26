// SPDX-License-Identifier: AGPL-3.0-or-later
import { onMount } from "solid-js";
import { t } from "../i18n";

interface Props {
  fileName: string;
  wrong: boolean;
  onSubmit: (password: string) => void;
  onCancel: () => void;
}

/** Asks for a document password. The value is never stored anywhere. */
export function PasswordDialog(props: Props) {
  let dialog!: HTMLDialogElement;
  let input!: HTMLInputElement;

  onMount(() => {
    dialog.showModal();
    input.focus();
  });

  return (
    <dialog
      ref={dialog}
      class="dialog"
      aria-labelledby="password-title"
      onCancel={(e) => {
        e.preventDefault();
        props.onCancel();
      }}
    >
      <form
        method="dialog"
        onSubmit={(e) => {
          e.preventDefault();
          const value = input.value;
          input.value = "";
          props.onSubmit(value);
        }}
      >
        <h2 id="password-title" class="dialog-title">
          {t("password.title")}
        </h2>
        <label class="dialog-label" for="password-input">
          {t("password.label", { name: props.fileName })}
        </label>
        <input
          ref={input}
          id="password-input"
          class="input"
          type="password"
          autocomplete="off"
          spellcheck={false}
          aria-invalid={props.wrong}
          aria-describedby={props.wrong ? "password-error" : undefined}
        />
        {props.wrong && (
          <p id="password-error" class="dialog-error" role="alert">
            {t("password.wrong")}
          </p>
        )}
        <div class="dialog-actions">
          <button type="button" class="btn" onClick={() => props.onCancel()}>
            {t("password.cancel")}
          </button>
          <button type="submit" class="btn btn-primary">
            {t("password.submit")}
          </button>
        </div>
      </form>
    </dialog>
  );
}
