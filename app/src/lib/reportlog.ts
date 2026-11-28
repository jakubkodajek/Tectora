// What a bug report carries about the session: the last steps (command
// names, result and duration, never their arguments) and the last notices
// the user was shown. A plain ring buffer, no reactivity; it leaves the
// browser only inside a report the user sends.

export const MAX_ACTIONS = 50;
export const MAX_NOTICES = 30;
const MAX_STRING = 300;
const MAX_DETAIL = 2000;

export type ActionKind = "command" | "edit" | "undo" | "redo" | "tool" | "open" | "nav";

export interface LoggedAction {
  ts_ms: number;
  kind: ActionKind | string;
  name: string;
  ok: boolean;
  code?: string | null;
  ms: number;
}

export interface LoggedNotice {
  ts_ms: number;
  kind: string;
  code?: string | null;
  title: string;
  detail?: string | null;
}

let actions: LoggedAction[] = [];
let notices: LoggedNotice[] = [];

const cut = (s: unknown, max: number) => {
  const text = typeof s === "string" ? s : s == null ? "" : String(s);
  return text.length > max ? text.slice(0, max - 1) + "…" : text;
};

function push<T>(list: T[], item: T, max: number): T[] {
  list.push(item);
  return list.length > max ? list.slice(list.length - max) : list;
}

export function logAction(a: Omit<LoggedAction, "ts_ms">) {
  actions = push(
    actions,
    {
      ts_ms: Date.now(),
      kind: cut(a.kind, MAX_STRING),
      name: cut(a.name, MAX_STRING),
      ok: !!a.ok,
      code: a.code == null ? null : cut(a.code, MAX_STRING),
      ms: Math.max(0, Math.round(Number(a.ms) || 0)),
    },
    MAX_ACTIONS,
  );
}

export function logNotice(n: Omit<LoggedNotice, "ts_ms">) {
  notices = push(
    notices,
    {
      ts_ms: Date.now(),
      kind: cut(n.kind, MAX_STRING),
      code: n.code == null ? null : cut(n.code, MAX_STRING),
      title: cut(n.title, MAX_STRING),
      detail: n.detail == null ? null : cut(n.detail, MAX_DETAIL),
    },
    MAX_NOTICES,
  );
}

/** Copies, oldest first. */
export const recentActions = (): LoggedAction[] => actions.map((a) => ({ ...a }));
export const recentNotices = (): LoggedNotice[] => notices.map((n) => ({ ...n }));

export function resetReportLog() {
  actions = [];
  notices = [];
}
