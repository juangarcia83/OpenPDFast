// SPDX-License-Identifier: AGPL-3.0-or-later
import { t } from "../i18n";
import { Dialog } from "./Dialog";

interface Props {
  fileName: string;
  wrong: boolean;
  onSubmit: (password: string) => void;
  onCancel: () => void;
}

/** Asks for a document password. The value is never stored anywhere. */
export function PasswordDialog(props: Props) {
  let input!: HTMLInputElement;
  return (
    <Dialog title={t("password.title")} onClose={props.onCancel} initialFocus={() => input}>
      <form
        class="dialog-body"
        onSubmit={(e) => {
          e.preventDefault();
          const value = input.value;
          input.value = "";
          props.onSubmit(value);
        }}
      >
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
          aria-describedby={props.wrong ? "password-error password-note" : "password-note"}
        />
        {props.wrong && (
          <p id="password-error" class="dialog-error" role="alert">
            {t("password.wrong")}
          </p>
        )}
        <p id="password-note" class="dialog-note">
          {t("password.note")}
        </p>
        <div class="dialog-actions">
          <button type="button" class="btn" onClick={() => props.onCancel()}>
            {t("password.cancel")}
          </button>
          <button type="submit" class="btn btn-primary">
            {t("password.submit")}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
