// A double click opens what its first press selected (review UI-008): the
// first click can open a detail pane that narrows the list, so the rows move
// and the second press lands on another row. The row of the first press is
// remembered and the double click goes to it.

/** How long after the first press a double click still belongs to it, ms. */
export const DOUBLE_CLICK_MS = 800;

export function firstPress<T>() {
  let first: { id: T; at: number } | null = null;
  return {
    /** A press on `id`; `count` is the event's click count (`MouseEvent.detail`). */
    press(id: T, count: number, at = performance.now()) {
      if (count <= 1) first = { id, at };
    },
    /** What a double click reported on `id` opens: the first press's row when it was just now. */
    target(id: T, at = performance.now()): T {
      const f = first;
      first = null;
      return f && at - f.at >= 0 && at - f.at <= DOUBLE_CLICK_MS ? f.id : id;
    },
  };
}

/** How long a button that replaced another under the pointer ignores presses, ms. */
export const FRESH_BUTTON_MS = 500;

/**
 * Whether a press on a destructive button must be ignored: the button
 * appeared under the pointer just now (the second click of a double click on
 * the button it replaced, UI-103), or the press is itself a double click's
 * second click (`count` = `MouseEvent.detail`).
 */
export function pressTooSoon(shownAt: number, count: number, at = performance.now()): boolean {
  return count >= 2 || at - shownAt < FRESH_BUTTON_MS;
}
