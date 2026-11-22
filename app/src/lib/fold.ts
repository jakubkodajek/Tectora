// The library search's folding of text, the same as the core's
// (`library::fold`, used by MCP `list_projects`): lower case, accents
// dropped, full-width letters and ligatures as plain ones, and the few
// letters with no decomposition (ß, æ, ł, ø, …) spelled out. So a word
// found by the AI's search is found by the library screen too.

/** Letters NFKD leaves alone, as the core folds them. */
const SPELLED: Record<string, string> = {
  ß: "ss",
  æ: "ae",
  œ: "oe",
  þ: "th",
  đ: "d",
  ð: "d",
  ħ: "h",
  ı: "i",
  ŀ: "l",
  ł: "l",
  ø: "o",
  ŧ: "t",
  ς: "σ",
};
const SPECIAL = new RegExp(`[${Object.keys(SPELLED).join("")}]`, "gu");

/** `s` folded for a search: compare folded words with a folded text. */
export function fold(s: string): string {
  return s
    .toLowerCase()
    .replace(SPECIAL, (c) => SPELLED[c])
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .toLowerCase();
}
