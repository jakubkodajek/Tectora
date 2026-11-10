// The user-visible name of a building (a part). Default names that nobody
// typed show as the localized "Budova {n}" (UX-010): the core's English
// defaults from templates and AI clients ("Main", "Part 3", "Part 2.1"),
// the editor's earlier defaults in any language ("Hlavní střecha", "Část 2",
// "Main roof", "Teil 2") and today's defaults saved in another language
// ("Building 2" in a Czech window). A number in the name is kept, so
// deleting a building does not renumber the others; a copy's " (2)" stays.
// A name the user or the AI typed shows as it is; the file keeps whatever
// is stored.
import { t } from "./i18n.svelte";

const FIRST = String.raw`Main|Main roof|Hlavní střecha|Hlavná strecha|Hauptdach`;
const NUMBERED = String.raw`Part|Část|Časť|Teil|Budova|Building|Gebäude`;
const DEFAULT = new RegExp(String.raw`^(?:(${FIRST})|(?:${NUMBERED}) (\d+(?:\.\d+)?))( \(\d+\))?$`);

/** True for a default name that nobody changed. */
export const isDefaultPartName = (name: string | null | undefined) => !!name && DEFAULT.test(name.trim());

/** The name to show for part `index` (from 0) whose stored name is `name`. */
export function partLabel(name: string | null | undefined, index: number): string {
  const m = name ? DEFAULT.exec(name.trim()) : null;
  if (name && !m) return name;
  const n = m?.[2] ?? (m?.[1] ? "1" : String(index + 1));
  return t("editor.partN", { n }) + (m?.[3] ?? "");
}
