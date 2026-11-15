// A dialog opened on one part (or dormer) of the job keeps its index. An
// outside save (AI, CLI) can remove or change that part while the dialog is
// open; the index then points at another part, and OK would act on it
// (ERRU-010). The anchor remembers how the part looked when the dialog
// opened, so the editor can tell that it is gone or changed.

/** The index of an object in a list of the job and its content when the dialog opened. */
export interface Anchor {
  index: number;
  key: string;
}

const keyOf = (v: unknown) => JSON.stringify(v ?? null);

/** Anchors a dialog on `list[index]`. */
export function anchor(list: readonly unknown[] | undefined, index: number): Anchor {
  return { index, key: keyOf(list?.[index]) };
}

/** The anchored object is still at its index, unchanged. */
export function stillThere(list: readonly unknown[] | undefined, a: Anchor): boolean {
  return !!list && a.index >= 0 && a.index < list.length && keyOf(list[a.index]) === a.key;
}
