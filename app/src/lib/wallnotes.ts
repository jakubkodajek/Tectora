// What the pending AI changes did to each wall's roof definition, as a short
// note the plan writes next to the highlighted wall ("40° → 35°", "40° →
// Gable", "Overhang 0,50 → 0,80 m"), so the colleague sees the change where
// it is and not only in the banner's text line (review UX-017). Display only.

import { num } from "./format";
import { t } from "./i18n.svelte";
import type { Change } from "./types";

const isVertical = (p: number) => Math.abs(p - 90) <= 0.001;
const deg = (v: number) => `${num(v).replace(/[.,]00$/, "")}°`;

/** The fields noted, in the order the note lists them. */
const FIELDS = ["pitch_deg", "eave_height", "overhang"] as const;

/** One field's text, from its value before the changes to its value after. */
function fieldNote(field: (typeof FIELDS)[number], a: number, b: number): string {
  if (field === "pitch_deg") {
    const side = (p: number) => (isVertical(p) ? t("edge.gable") : deg(p));
    return `${side(a)} → ${side(b)}`;
  }
  return `${t(field === "overhang" ? "edge.overhang" : "edge.eaveHeight")} ${num(a)} → ${num(b)} m`;
}

/**
 * Notes per wall, keyed "part:edge" like `changedEdges`. `changes` is newest
 * first (as the review lists the AI steps): a field changed in several steps
 * shows from its value before the first step to its value after the last,
 * and a field that ended where it started shows nothing.
 */
export function wallNotes(changes: Change[]): Map<string, string> {
  // key -> field -> [before (oldest), after (newest)]
  const span = new Map<string, Map<string, [number, number]>>();
  for (const c of changes) {
    if (c.kind !== "modified" || c.part === undefined || c.edge === undefined || c.item !== undefined) continue;
    if (!(FIELDS as readonly string[]).includes(c.field)) continue;
    const [a, b] = [c.before, c.after];
    if (typeof a !== "number" || typeof b !== "number" || !Number.isFinite(a) || !Number.isFinite(b)) continue;
    const key = `${c.part}:${c.edge}`;
    const fields = span.get(key) ?? new Map<string, [number, number]>();
    const was = fields.get(c.field);
    // Newest first: the first seen keeps its "after", later ones push "before" back.
    fields.set(c.field, was ? [a, was[1]] : [a, b]);
    span.set(key, fields);
  }
  const out = new Map<string, string>();
  for (const [key, fields] of span) {
    const parts = FIELDS.flatMap((f) => {
      const s = fields.get(f);
      return s && Math.abs(s[0] - s[1]) > 1e-9 ? [fieldNote(f, s[0], s[1])] : [];
    });
    if (parts.length) out.set(key, parts.join(" · "));
  }
  return out;
}
