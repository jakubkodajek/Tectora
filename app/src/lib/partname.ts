// The user-visible name of a building (a part). A job made from a template or
// by an AI client carries the core's untouched English default ("Main",
// "Main (2)", "Part 3", "Part 2.1"); those show as the localized "Budova {n}"
// (UX-010). A name the user or the AI typed shows as it is; the file keeps
// whatever is stored.
import { t } from "./i18n.svelte";

const CORE_DEFAULT = /^(Main|Part \d+(\.\d+)?)( \(\d+\))?$/;

/** True for a name the core gave and nobody changed. */
export const isDefaultPartName = (name: string | null | undefined) => !!name && CORE_DEFAULT.test(name);

/** The name to show for part `index` (from 0) whose stored name is `name`. */
export function partLabel(name: string | null | undefined, index: number): string {
  if (!name || CORE_DEFAULT.test(name)) return t("editor.partN", { n: index + 1 });
  return name;
}
