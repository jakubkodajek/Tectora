// The app's translations, read from src/lib/i18n.svelte.ts (a runes module
// that plain Node cannot import), so tests can find UI text in any locale.
import fs from "node:fs";
import path from "node:path";
import { APP_DIR } from "./env";
import type { Locale } from "./fixtures";

type Dict = Record<string, string>;

function load(): Record<Locale, Dict> {
  const src = fs.readFileSync(path.join(APP_DIR, "src", "lib", "i18n.svelte.ts"), "utf8");
  const start = src.indexOf("const cs: Dict = {");
  const end = src.indexOf("export const DICTS");
  if (start < 0 || end < 0) throw new Error("i18n.svelte.ts layout changed; update e2e/i18n.ts");
  const body = src.slice(start, end).replace(/: Dict =/g, " =");
  return new Function(`${body}; return { cs, sk, en, de };`)() as Record<Locale, Dict>;
}

export const DICTS = load();
export const LOCALES: Locale[] = ["cs", "sk", "en", "de"];

/** The text of `key` in `locale`, with {placeholders} filled in. */
export function tr(locale: Locale, key: string, vars: Record<string, string | number> = {}): string {
  const s = DICTS[locale][key];
  if (s === undefined) throw new Error(`no translation key ${key}`);
  return s.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`));
}

/** Czech text of `key` (the default locale of the tests). */
export const cs = (key: string, vars?: Record<string, string | number>) => tr("cs", key, vars);

/** An exact-match regex for a translated label (buttons keep icons next to text). */
export const re = (s: string) => new RegExp(`^\\s*${s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*$`);
