// What the job library says of a newer version of the app (UpdateBanner).

import type { AvailableUpdate, UpdateProgress } from "./types";

type Translate = (key: string, params?: Record<string, string | number>) => string;

/** The question that confirms installing `update`, with what its release says changed. */
export function installQuestion(t: Translate, update: AvailableUpdate): string {
  const question = t("update.confirm", { version: update.version });
  return update.notes ? `${question}\n\n${t("update.notes")}\n${update.notes}` : question;
}

/** How installing `version` goes, given how much of it has been downloaded: null before the first report. */
export function installStatus(t: Translate, version: string, progress: UpdateProgress | null): string {
  const total = progress?.total;
  if (!progress || !total) return t("update.downloading", { version });
  if (progress.downloaded >= total) return t("update.installing", { version });
  return t("update.downloadingPercent", { version, percent: Math.floor((progress.downloaded * 100) / total) });
}
