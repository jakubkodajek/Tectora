// App-wide notifications (toasts). Errors stay until closed or for a while
// longer than info messages, repeat errors are merged, and every error
// keeps its original text under "Details".

import { describeError, type Action } from "./errors";

export interface Toast {
  id: number;
  kind: "info" | "warning" | "error";
  /** Machine code shown small (e.g. permission_denied). */
  code?: string;
  title: string;
  message?: string;
  detail?: string;
  /** How many times the same message came in. */
  count: number;
}

export const toasts = $state<Toast[]>([]);

const timers = new Map<number, ReturnType<typeof setTimeout>>();
let next = 1;
const MAX = 4;
const LIFETIME = { info: 4000, warning: 9000, error: 12000 };

function schedule(id: number, kind: Toast["kind"]) {
  clearTimeout(timers.get(id));
  timers.set(
    id,
    setTimeout(() => dismiss(id), LIFETIME[kind]),
  );
}

export function dismiss(id: number) {
  clearTimeout(timers.get(id));
  timers.delete(id);
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}

/** Keeps a toast open while the pointer is over it. */
export function hold(id: number, on: boolean) {
  const toast = toasts.find((t) => t.id === id);
  if (!toast) return;
  if (on) {
    clearTimeout(timers.get(id));
    timers.delete(id);
  } else {
    schedule(id, toast.kind);
  }
}

export function notify(t: Omit<Toast, "id" | "count">): number {
  const same = toasts.find((x) => x.kind === t.kind && x.title === t.title && x.message === t.message);
  if (same) {
    same.count += 1;
    same.detail = t.detail ?? same.detail;
    schedule(same.id, same.kind);
    return same.id;
  }
  const id = next++;
  toasts.push({ ...t, id, count: 1 });
  while (toasts.length > MAX) dismiss(toasts[0].id);
  schedule(id, t.kind);
  return id;
}

export const info = (title: string) => notify({ kind: "info", title });

/** Shows a translated error for anything thrown by `action`. */
export function notifyError(e: unknown, action?: Action): number {
  const d = describeError(e, action);
  console.error(`[tectora] ${action ?? "error"}:`, e);
  return notify({
    kind: d.code === "watcher" || d.code === "settings_unreadable" ? "warning" : "error",
    code: d.code,
    title: d.title,
    message: d.message,
    detail: d.detail,
  });
}
