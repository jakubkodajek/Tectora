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
