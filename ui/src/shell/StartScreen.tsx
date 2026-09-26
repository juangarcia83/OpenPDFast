// SPDX-License-Identifier: AGPL-3.0-or-later
import { createResource, For, Show } from "solid-js";
import { commands, type RecentDocument } from "../bindings";
import { IconClose, IconFile, IconOpen } from "../design/icons";
import { t } from "../i18n";

function relativeTime(seconds: number): string {
  const diff = Math.max(0, Date.now() / 1000 - seconds);
  if (diff < 60) return t("time.justNow");
  if (diff < 3600) return t("time.minutes", { n: String(Math.floor(diff / 60)) });
  if (diff < 86400) return t("time.hours", { n: String(Math.floor(diff / 3600)) });
  return t("time.days", { n: String(Math.floor(diff / 86400)) });
}

const folderOf = (path: string) => path.replace(/[\\/][^\\/]*$/, "");

interface Props {
  onOpenDialog: () => void;
  onOpenPath: (path: string) => void;
}

export function StartScreen(props: Props) {
  const [recent, { mutate }] = createResource(async () => {
    const r = await commands.recentDocuments();
    return r.status === "ok" ? r.data : [];
  });

  const forget = (doc: RecentDocument) => {
    mutate((list) => list?.filter((d) => d.path !== doc.path));
    void commands.forgetDocument(doc.path);
  };

  return (
    <div class="start">
      <section class="start-hero">
        <img class="start-logo" src="/app-icon.svg" alt="" width="72" height="72" />
        <h1 class="start-title">{t("app.title")}</h1>
        <p class="start-tagline">{t("app.tagline")}</p>
        <button
          type="button"
          class="btn btn-primary btn-large"
          onClick={() => props.onOpenDialog()}
        >
          <IconOpen /> {t("start.open")}
        </button>
        <p class="start-hint">{t("start.drop")}</p>
      </section>

      <section class="start-recent" aria-labelledby="recent-title">
        <h2 id="recent-title" class="section-title">
          {t("start.recent")}
        </h2>
        <Show
          when={(recent() ?? []).length > 0}
          fallback={<p class="start-empty">{t("start.recentEmpty")}</p>}
        >
          <ul class="recent-list">
            <For each={recent()}>
              {(doc) => (
                <li class="recent-item">
                  <button
                    type="button"
                    class="recent-open"
                    onClick={() => props.onOpenPath(doc.path)}
                    title={doc.path}
                  >
                    <IconFile size={22} />
                    <span class="recent-text">
                      <span class="recent-name">{doc.fileName}</span>
                      <span class="recent-meta">
                        {doc.pageCount
                          ? t("start.resume", {
                              page: String(doc.page + 1),
                              count: String(doc.pageCount),
                            })
                          : t("start.resumeNoCount", { page: String(doc.page + 1) })}
                        {" · "}
                        {relativeTime(doc.lastOpened)}
                        {" · "}
                        {folderOf(doc.path)}
                      </span>
                    </span>
                  </button>
                  <button
                    type="button"
                    class="btn btn-icon recent-forget"
                    aria-label={t("start.forget", { name: doc.fileName })}
                    title={t("start.forget", { name: doc.fileName })}
                    onClick={() => forget(doc)}
                  >
                    <IconClose size={16} />
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
        <p class="start-tip">{t("start.tips", { key: "F1" })}</p>
      </section>
    </div>
  );
}
