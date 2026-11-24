// The library search's folding of text, exactly the core's (`library::fold`,
// used by MCP `list_projects`), so a word found by the AI's search is found
// by the library screen too and the other way round (audit 2026-11-22
// UX-503; both are tested with `tests/fixtures/parity/fold_cases.json`):
// composed (NFC) and lower case, the accents of letters dropped (a letter
// whose canonical decomposition is a base letter and marks U+0300 to U+036F
// becomes that base letter, a mark of that range left alone is dropped),
// full-width letters and digits (U+FF01 to U+FF5E) as the plain ones, and
// ligatures and the few letters with no decomposition (ß, æ, ł, ø, …)
// spelled out. Nothing else: compatibility characters (m², Ⅻ, ①) stay as
// they are, as in the core, which has no compatibility table. A code point
// NFC changes on its own (the Greek oxia vowels, U+2000, the CJK
// compatibility ideographs) is changed by both; the core has their table
// (`nfc_singletons.rs`). `tests/fixtures/parity/fold_all.json` holds both
// to the same fold over every code point (TEST-608).

/** Letters and ligatures the core spells out (`library::fold_into`, `fold_char`). */
const SPELLED: Record<string, string> = {
  ß: "ss",
  æ: "ae",
  œ: "oe",
  þ: "th",
  "\u{fb00}": "ff",
  "\u{fb01}": "fi",
  "\u{fb02}": "fl",
  "\u{fb03}": "ffi",
  "\u{fb04}": "ffl",
  "\u{fb05}": "st",
  "\u{fb06}": "st",
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

/** A combining mark the core's composition table knows (U+0300 to U+036F). */
const MARK = /^[̀-ͯ]$/u;
const MARKS = /^[̀-ͯ]+$/u;

/** The base letter of `c` when it is a letter with marks of that range
 * only. Below U+2000 only, where the core's composition table has every
 * such letter (Latin, Greek, Cyrillic): a symbol with a stroke (`≠`, `↚`)
 * stays as it is in the core (audit 2026-11-22 TEST-608). */
function baseLetter(c: string): string {
  if (c.codePointAt(0)! >= 0x2000) return c;
  const d = c.normalize("NFD");
  const base = String.fromCodePoint(d.codePointAt(0)!);
  const rest = d.slice(base.length);
  return rest && MARKS.test(rest) ? base : c;
}

/** `s` folded for a search: compare folded words with a folded text. */
export function fold(s: string): string {
  let out = "";
  for (const ch of s.normalize("NFC").toLowerCase()) {
    if (MARK.test(ch)) continue;
    const code = ch.codePointAt(0)!;
    const c = code >= 0xff01 && code <= 0xff5e ? String.fromCodePoint(code - 0xfee0) : ch;
    out += foldChar(c);
  }
  return out;
}

/** `c` spelled out or without its marks; the base of a letter with marks
 * is folded again (`ǽ` is `æ`, spelled `ae`; `ǿ` is `ø`, which is `o`), as
 * the core's `fold_into` does (audit 2026-11-22 HUNT-602). */
function foldChar(c: string): string {
  for (let i = 0; i < 4; i++) {
    const spelled = SPELLED[c];
    if (spelled !== undefined) return spelled;
    const base = baseLetter(c);
    if (base === c) break;
    c = base;
  }
  return c;
}

/** The core's white space (`char::is_whitespace`) and U+FEFF: `\s` and U+0085. */
const SPACE = /[\s\u0085]+/u;

/** The words of a folded search text, split as the core's
 * `library::search_words` splits them (TEST-608: `\s` splits at U+FEFF and
 * not at U+0085, the core the other way round). */
export function searchWords(folded: string): string[] {
  return folded.split(SPACE).filter(Boolean);
}
