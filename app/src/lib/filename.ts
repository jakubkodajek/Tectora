// A job's name as a file name for a save dialog. Job names are written by
// people ("Novák / Brno 2026:117"): "/" is a folder on macOS, ":" and the
// other reserved characters are refused by Windows, which also strips a
// trailing dot or space and keeps device names (CON, NUL, COM1, …) for
// itself. One cleaner for every export, so both platforms get a name they
// take (review UX-006).

/** Windows device names, which no file may be called (with any extension). */
const RESERVED = /^(con|prn|aux|nul|com[0-9¹²³]|lpt[0-9¹²³])$/i;

/** Longest name proposed, characters: room for the extension inside the 255 both systems allow. */
const MAX_NAME = 120;

/**
 * `name` made safe as a file name on macOS and Windows, without extension;
 * `fallback` when nothing usable is left.
 */
export function safeFileName(name: string | null | undefined, fallback = "job"): string {
  let s = (name ?? "")
    // Reserved characters and control characters become one "_".
    // eslint-disable-next-line no-control-regex
    .replace(/[\\/:*?"<>|\u0000-\u001f\u007f]+/g, "_")
    .replace(/\s+/g, " ")
    .trim();
  if (s.length > MAX_NAME) s = s.slice(0, MAX_NAME).trim();
  // Windows drops trailing dots and spaces; a leading dot hides the file on macOS.
  s = s.replace(/[. ]+$/, "").replace(/^[. ]+/, "");
  if (!s || /^_+$/.test(s)) return fallback;
  if (RESERVED.test(s.split(".")[0])) s = `_${s}`;
  return s;
}
