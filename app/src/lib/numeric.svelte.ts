// One behaviour for every number field in the app.
//
// Native `<input type="number">` behaves differently in each web view: WebKit
// (macOS) shows and parses the number in the system locale ("2,5"), Chromium
// (Windows WebView2, tests) only takes a dot, and a value typed in either is
// lost when a click elsewhere changes what the field edits before the field
// has sent its `change` event. So number fields are plain text fields driven
// by this action:
//
// - accepts "2,5", "2.5" and spaces, shows the number in the UI language;
// - commits on Enter, Tab / blur, and on a pointer press anywhere outside the
//   field *before* that press is handled (so clicking the next wall stores the
//   value on the wall it was typed for, not on the newly selected one);
// - Escape restores the stored value; an invalid entry is restored and marked;
// - ArrowUp / ArrowDown and the small ▲▼ buttons step by `step`;
// - while the user types, outside updates (autosave, AI edits) never replace
//   the typed text.

import { untrack } from "svelte";
import { numberLocale, t } from "./i18n.svelte";

/** Width the ▲▼ buttons take from the field's right padding, px. */
const SPIN_W = 15;

export interface NumericOptions {
  /** The stored value; null or undefined shows an empty field. */
  value: number | null | undefined;
  /**
   * Called with a parsed, in-range value that differs from `value`.
   * Return `false` to reject it: the field shows the stored value again.
   */
  onchange: (v: number) => unknown;
  /** Step of the arrow keys and the ▲▼ buttons (default 1). */
  step?: number;
  /** Inclusive bounds; values outside are rejected, steps are clamped. */
  min?: number;
  max?: number;
  /** Fixed number of decimals shown (default: up to 3, trailing zeros cut). */
  digits?: number;
  /** Called when the field is emptied and committed; without it, empty restores the value. */
  onclear?: () => unknown;
  /** Also commit every valid keystroke (live calculators). */
  live?: boolean;
  /** Hide the ▲▼ buttons (fields that are not stepped, e.g. coordinates in a dialog). */
  spinner?: boolean;
}

/** Parses a typed number: comma or dot as the decimal separator, spaces ignored. */
export function parseNumber(text: string): number | null {
  const s = text
    .trim()
    .replace(/[\s  ]/g, "")
    .replace(/−/g, "-")
    .replace(",", ".");
  if (!/^[-+]?(\d+\.?\d*|\.\d+)(e[-+]?\d+)?$/i.test(s)) return null;
  const v = Number(s);
  return Number.isFinite(v) ? v : null;
}

/** Formats a value for a field in the given locale, without digit grouping. */
export function formatNumber(v: number | null | undefined, locale: string, digits?: number): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return "";
  return new Intl.NumberFormat(locale, {
    useGrouping: false,
    minimumFractionDigits: digits ?? 0,
    maximumFractionDigits: digits ?? 3,
  }).format(v);
}

/** Decimals of a step, so 0.05 steps give 2.35 and not 2.3500000000000001. */
function decimalsOf(n: number): number {
  const s = String(n);
  const e = s.match(/e-(\d+)$/);
  if (e) return +e[1];
  const i = s.indexOf(".");
  return i < 0 ? 0 : s.length - i - 1;
}

/** `base` moved by `dir` steps, rounded to the step's decimals and clamped. */
export function stepValue(base: number, dir: number, step: number, min?: number, max?: number): number {
  const d = Math.max(decimalsOf(step), decimalsOf(base));
  let v = +(base + dir * step).toFixed(Math.min(d, 10));
  if (min !== undefined && v < min) v = min;
  if (max !== undefined && v > max) v = max;
  return v;
}

export function numeric(input: HTMLInputElement, initial: NumericOptions) {
  let opts = initial;
  let dirty = false;
  // Marks the field as one of ours, and whether it holds typed text not
  // stored yet (follow mode waits for that, leave.ts `isTyping`).
  input.dataset.field = "";
  const setDirty = (d: boolean) => {
    dirty = d;
    input.toggleAttribute("data-typing", d);
  };
  let invalid = false;

  input.type = "text";
  input.inputMode = "decimal";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.classList.add("num-input");

  const fmt = (v: number | null | undefined) => formatNumber(v, numberLocale(), opts.digits);

  let ownTitle = input.title;
  let clearTimer: ReturnType<typeof setTimeout> | undefined;

  /** Why `text` cannot be stored, for the tooltip; null when it can. */
  function reason(text: string): string | null {
    const v = parseNumber(text);
    if (v === null) return t("num.nan");
    const f = (x: number) => formatNumber(x, numberLocale());
    const { min, max } = opts;
    if ((min !== undefined && v < min) || (max !== undefined && v > max)) {
      if (min !== undefined && max !== undefined) return t("num.range", { min: f(min), max: f(max) });
      return min !== undefined ? t("num.min", { min: f(min) }) : t("num.max", { max: f(max!) });
    }
    return null;
  }

  /** Marks the field invalid with the reason as its tooltip (and for screen readers). */
  function setInvalid(why: string | null) {
    clearTimeout(clearTimer);
    invalid = why !== null;
    if (why !== null) {
      input.setAttribute("aria-invalid", "true");
      input.title = why;
    } else {
      input.removeAttribute("aria-invalid");
      input.title = ownTitle;
    }
  }

  function render() {
    if (dirty) return;
    const text = fmt(opts.value);
    if (input.value !== text) input.value = text;
  }

  /** A value the field may store, or null. */
  function accept(v: number | null): number | null {
    if (v === null) return null;
    if (opts.min !== undefined && v < opts.min) return null;
    if (opts.max !== undefined && v > opts.max) return null;
    return v;
  }

  function store(v: number): boolean {
    if (opts.value === v) return true;
    return opts.onchange(v) !== false;
  }

  /** Stores the typed text if it changed; restores the stored value if it is not a number we take. */
  function commit() {
    if (!dirty) return;
    setDirty(false);
    if (opts.onclear && input.value.trim() === "") {
      setInvalid(null);
      opts.onclear();
      return;
    }
    const text = input.value;
    const v = accept(parseNumber(text));
    const ok = v !== null && store(v);
    // The stored value may arrive later (parent re-render); show the typed
    // number now, or the stored one when it was refused.
    input.value = ok ? fmt(v) : fmt(opts.value);
    if (ok || text.trim() === "") {
      setInvalid(null);
    } else {
      // Say why for a moment, then the restored value is plain again.
      setInvalid(reason(text) ?? t("num.refused"));
      clearTimer = setTimeout(() => !dirty && setInvalid(null), 2500);
    }
  }

  function stepBy(dir: number) {
    if (input.disabled || input.readOnly) return;
    const typed = dirty ? parseNumber(input.value) : null;
    const base = typed ?? opts.value ?? opts.min ?? 0;
    const v = stepValue(base, dir, opts.step ?? 1, opts.min, opts.max);
    setDirty(false);
    setInvalid(null);
    if (store(v)) input.value = fmt(v);
    else input.value = fmt(opts.value);
  }

  function onInput() {
    setDirty(true);
    const text = input.value.trim();
    // A lone "-" or "," is a number being typed, not an error yet.
    const partial = /^[-+\u2212]?[.,]?$/.test(text);
    const v = accept(parseNumber(text));
    setInvalid(v === null && !partial ? reason(text) : null);
    if (opts.live && v !== null) {
      // Live fields store as they go but keep the text as typed ("2," stays).
      if (opts.value !== v) opts.onchange(v);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      // Stored before a dialog's own Enter (OK) sees the event.
      commit();
      input.select();
    } else if (e.key === "Escape" && dirty) {
      setDirty(false);
      setInvalid(null);
      render();
      e.preventDefault();
      e.stopPropagation();
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      stepBy(e.key === "ArrowUp" ? 1 : -1);
    }
  }

  // A press anywhere outside commits first, before the press itself selects
  // another wall, closes a bar or opens a dialog (capture phase on window).
  function onOutsidePress(e: Event) {
    const target = e.target as Node | null;
    if (target && (input.contains(target) || spin?.contains(target))) return;
    commit();
  }

  function onFocus() {
    window.addEventListener("pointerdown", onOutsidePress, true);
  }

  function onBlur() {
    window.removeEventListener("pointerdown", onOutsidePress, true);
    commit();
  }

  // ▲▼ buttons next to the unit, like the native number spinner.
  let spin: HTMLSpanElement | null = null;
  let resize: ResizeObserver | null = null;
  let basePad = 0;
  function buildSpinner() {
    if (opts.spinner === false || spin) return;
    spin = document.createElement("span");
    spin.className = "num-spin";
    spin.setAttribute("aria-hidden", "true");
    for (const [dir, label] of [
      [1, "num.up"],
      [-1, "num.down"],
    ] as const) {
      const b = document.createElement("button");
      b.type = "button";
      b.tabIndex = -1;
      b.dataset.label = label;
      b.dataset.dir = String(dir);
      b.innerHTML =
        dir > 0
          ? '<svg viewBox="0 0 10 6" width="9" height="6"><path d="M1 5l4-4 4 4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>'
          : '<svg viewBox="0 0 10 6" width="9" height="6"><path d="M1 1l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>';
      // Keep the focus (and the typed text) in the field.
      b.addEventListener("mousedown", (e) => e.preventDefault());
      b.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        // Arrow keys after a click on ▲▼ step this field, not the selected object.
        if (document.activeElement !== input) input.focus({ preventScroll: true });
        stepBy(dir);
      });
      spin.appendChild(b);
    }
    input.insertAdjacentElement("afterend", spin);
    const parent = input.parentElement;
    if (parent && getComputedStyle(parent).position === "static") parent.style.position = "relative";
    // Sit just inside the field's right edge, left of a unit drawn over the
    // field's right padding (the bars' "m" / "°").
    basePad = parseFloat(getComputedStyle(input).paddingRight) || 0;
    input.style.paddingRight = `${basePad + SPIN_W}px`;
    placeSpinner();
    if (parent) {
      resize = new ResizeObserver(placeSpinner);
      resize.observe(parent);
    }
  }

  function placeSpinner() {
    const parent = input.parentElement;
    if (!spin || !parent) return;
    const right = parent.clientWidth - (input.offsetLeft + input.offsetWidth) + basePad - 2;
    spin.style.right = `${Math.max(0, right)}px`;
  }

  function translateSpinner() {
    spin?.querySelectorAll("button").forEach((b) => (b.title = t(b.dataset.label ?? "")));
  }

  function syncDisabled() {
    spin?.querySelectorAll("button").forEach((b) => (b.disabled = input.disabled));
  }

  input.addEventListener("input", onInput);
  input.addEventListener("keydown", onKeydown);
  input.addEventListener("focus", onFocus);
  input.addEventListener("blur", onBlur);
  buildSpinner();
  render();
  syncDisabled();

  // The UI language changes the decimal separator; disabled follows the input.
  const stop = $effect.root(() => {
    $effect(() => {
      numberLocale();
      untrack(() => {
        render();
        syncDisabled();
        translateSpinner();
      });
    });
  });
  const observer = new MutationObserver(syncDisabled);
  observer.observe(input, { attributes: true, attributeFilter: ["disabled"] });

  return {
    update(next: NumericOptions) {
      opts = next;
      if (!dirty && invalid && fmt(opts.value) !== input.value) setInvalid(null);
      render();
      syncDisabled();
    },
    destroy() {
      window.removeEventListener("pointerdown", onOutsidePress, true);
      clearTimeout(clearTimer);
      stop();
      observer.disconnect();
      resize?.disconnect();
      spin?.remove();
    },
  };
}
