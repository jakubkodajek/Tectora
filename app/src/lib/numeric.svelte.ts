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
import { anyWriterOpen } from "./dragsession";

/** Width the ▲▼ buttons take from the field's right padding, px. */
const SPIN_W = 15;

/**
 * Largest absolute value any field takes: the core refuses every number
 * beyond it (`model::MAX_ABS_VALUE`, 100 km in metres), so a field without
 * its own bounds still refuses and explains it before anything is saved.
 */
export const MAX_VALUE = 1e5;

/** The bounds a field enforces: its own, never wider than ±MAX_VALUE. */
export function fieldBounds(min?: number, max?: number): { min: number; max: number } {
  return { min: Math.max(min ?? -MAX_VALUE, -MAX_VALUE), max: Math.min(max ?? MAX_VALUE, MAX_VALUE) };
}

export interface NumericOptions {
  /** The stored value; null or undefined shows an empty field. */
  value: number | null | undefined;
  /**
   * Called with a parsed, in-range value that differs from `value`.
   * Return `false` to reject it: the field shows the stored value again.
   * Return a string to reject it with that reason (shown as the tooltip).
   * A Promise of either (a value the core checks) does the same once it
   * settles, unless the user typed again meanwhile (review ERR-003).
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
  /** Called with the reason when a typed value is refused (out of range, not a number). */
  onrefuse?: (why: string) => unknown;
  /** Step with ArrowUp / ArrowDown (default true); false leaves the keys to the page. */
  arrows?: boolean;
  /** Told when the typed text becomes invalid (the reason) or valid again (null). */
  onvalidity?: (reason: string | null) => void;
  /**
   * Which item the field edits now, when one input is reused for several
   * (a bar that switches from chimney 1 to chimney 2): when it changes, the
   * field forgets what it sent and stepped for the item before, and an
   * answer for that item no longer marks the field (UI-403).
   */
  key?: string | number;
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

function isThenable(r: unknown): r is PromiseLike<unknown> {
  return typeof r === "object" && r !== null && typeof (r as PromiseLike<unknown>).then === "function";
}

/** A burst of arrow steps: the value it started from, the last step asked for and when. */
export interface Stepped {
  from: number | null | undefined;
  to: number;
  at: number;
}

/** How long a step whose value has not come back yet stays the base of the next, ms. */
export const STEP_WAIT_MS = 1500;

/**
 * The value the next arrow step goes on from while earlier steps of the
 * burst are still with the core, or null to go on from the stored value.
 * The stored value may be any step of the burst that came back meanwhile
 * (each is checked in turn), so a stored value on the way from `from` to
 * `to` still waits for `to` (UI-201). Only for a moment: a step the core
 * refused must not stay the base.
 */
export function waitingStep(stepped: Stepped | null, value: number | null | undefined, now: number): number | null {
  if (!stepped || now - stepped.at >= STEP_WAIT_MS || value === stepped.to) return null;
  if (value === stepped.from) return stepped.to;
  if (value == null || stepped.from == null) return null;
  const lo = Math.min(stepped.from, stepped.to);
  const hi = Math.max(stepped.from, stepped.to);
  return value >= lo && value <= hi ? stepped.to : null;
}

/**
 * Whether a committed value `v` goes to `onchange`: when it differs from
 * the stored value, or, while an earlier value is still with the core
 * (`out`), when it differs from that one. So ArrowUp then ArrowDown, or the
 * old value typed back, while the first answer is out sends the old value
 * after it and the field keeps the last value the user set (UI-307); the
 * same value committed twice is sent once.
 */
export function toSend(v: number, stored: number | null | undefined, out: number | null): boolean {
  return out !== null ? out !== v : stored !== v;
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
  /** The mark is a later refusal that stays until the next input (UX-401). */
  let stays = false;

  input.type = "text";
  input.inputMode = "decimal";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.classList.add("num-input");

  const fmt = (v: number | null | undefined) => formatNumber(v, numberLocale(), opts.digits);

  let ownTitle = input.title;
  let clearTimer: ReturnType<typeof setTimeout> | undefined;
  /** Counts item switches (`key`): an answer for an earlier item is not this field's any more. */
  let item = 0;

  /** Why `text` cannot be stored, for the tooltip; null when it can. */
  function reason(text: string): string | null {
    const v = parseNumber(text);
    if (v === null) return t("num.nan");
    const f = (x: number) => formatNumber(x, numberLocale());
    const { min, max } = fieldBounds(opts.min, opts.max);
    if (v < min || v > max) return t("num.range", { min: f(min), max: f(max) });
    return null;
  }

  /** Marks the field invalid with the reason as its tooltip (and for screen readers). */
  function setInvalid(why: string | null) {
    clearTimeout(clearTimer);
    if (invalid !== (why !== null) || (why !== null && input.title !== why)) opts.onvalidity?.(why);
    invalid = why !== null;
    stays = false;
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
    // While a burst of steps is with the core, the field shows the newest
    // step, not each earlier step as it comes back (UX-403); as long as
    // the newest step itself is still out, also past STEP_WAIT_MS.
    const lastOut = stepped !== null && out?.v === stepped.to;
    const waiting = waitingStep(stepped, opts.value, lastOut ? stepped!.at : performance.now());
    // A typed value still with the core is the field's last word: an
    // earlier step coming back meanwhile is not shown (TEST-502, N12).
    const typed = stepped === null && out !== null ? out.v : null;
    const text = fmt(waiting ?? typed ?? opts.value);
    if (input.value !== text) input.value = text;
  }

  /** A value the field may store, or null. */
  function accept(v: number | null): number | null {
    if (v === null) return null;
    const { min, max } = fieldBounds(opts.min, opts.max);
    return v < min || v > max ? null : v;
  }

  /** Why the last stored value was refused, when `onchange` said so. */
  let refusal: string | null = null;

  /** The last value sent whose answer is still out (a value the core checks). */
  let out: { v: number } | null = null;

  function store(v: number): boolean {
    refusal = null;
    if (!toSend(v, opts.value, out?.v ?? null)) return true;
    const r = opts.onchange(v);
    if (isThenable(r)) {
      const mine = (out = { v });
      // A refusal first (it shows the stored value and says why), then the
      // field lets go of the value; a burst of steps shows what it now may.
      later(r, v, mine);
      const settle = () => {
        if (out !== mine) return;
        out = null;
        render();
      };
      r.then(settle, settle);
    }
    if (typeof r === "string") refusal = r;
    return r !== false && typeof r !== "string";
  }

  /**
   * Marks the field refused with `why` and shows the stored value again.
   * A value refused at once (out of range) is marked for a moment, while
   * the user still looks at the field; one the editor refused later (the
   * core, the item changed meanwhile) stays marked until the user edits
   * the field again or it is switched to another item, so a lost value
   * stays visible (UX-401).
   */
  function refuse(why: string, stay = false) {
    input.value = fmt(opts.value);
    setInvalid(why);
    stays = stay;
    opts.onrefuse?.(why);
    if (!stay) clearTimer = setTimeout(() => !dirty && setInvalid(null), 2500);
  }

  /**
   * An answer that comes later: the typed number is shown meanwhile; a
   * refusal puts the stored value back and says why, so a refused value
   * never stays in the field as if stored.
   */
  function later(p: PromiseLike<unknown>, v: number, sent: { v: number }) {
    const mine = item;
    // Still the field's last word for this item: nothing typed or sent
    // since. Not whether the field still shows `v`: a re-render (an AI
    // save meanwhile) shows the stored value again, and the refusal must
    // still be marked (UX-401).
    const still = () => mine === item && !dirty && out === sent;
    const refused = (why: string) => {
      if (!still()) return;
      // A refused step is no base of the next one, nor what the field shows.
      if (stepped?.to === v) stepped = null;
      refuse(why, true);
    };
    p.then(
      (r) => {
        if (r === false || typeof r === "string") refused(typeof r === "string" ? r : t("num.refused"));
      },
      () => refused(t("num.refused")),
    );
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
    // A typed value ends a burst of steps: it is what the field shows now.
    if (v !== null) stepped = null;
    const ok = v !== null && store(v);
    // The stored value may arrive later (parent re-render); show the typed
    // number now, or the stored one when it was refused.
    input.value = ok ? fmt(v) : fmt(opts.value);
    if (ok || text.trim() === "") {
      setInvalid(null);
    } else {
      // Say why for a moment, then the restored value is plain again.
      refuse(reason(text) ?? refusal ?? t("num.refused"));
    }
  }

  // The last step stored while its value had not come back yet (a field
  // whose value returns from the core): the next step goes on from it, so
  // four quick arrow presses make four steps (review UI-002, UI-201).
  let stepped: Stepped | null = null;
  let stepTimer: ReturnType<typeof setTimeout> | undefined;

  function stepBy(dir: number) {
    if (input.disabled || input.readOnly) return;
    const typed = dirty ? parseNumber(input.value) : null;
    // Only for a moment: a step the core refused must not stay the base.
    const now = performance.now();
    const waiting = waitingStep(stepped, opts.value, now);
    const base = typed ?? waiting ?? opts.value ?? opts.min ?? 0;
    const { min, max } = fieldBounds(opts.min, opts.max);
    const v = stepValue(base, dir, opts.step ?? 1, min, max);
    setDirty(false);
    setInvalid(null);
    // A burst of steps keeps the value it started from.
    const from = waiting !== null && stepped ? stepped.from : opts.value;
    if (store(v)) {
      input.value = fmt(v);
      stepped = { from, to: v, at: now };
      // Once the burst is over, the field shows what is stored.
      clearTimeout(stepTimer);
      stepTimer = setTimeout(render, STEP_WAIT_MS);
    } else input.value = fmt(opts.value);
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
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && opts.arrows !== false) {
      e.preventDefault();
      stepBy(e.key === "ArrowUp" ? 1 : -1);
    }
  }

  // A press anywhere outside commits first, before the press itself selects
  // another wall, closes a bar or opens a dialog (capture phase on window).
  function onOutsidePress(e: Event) {
    // Not while a writer of the job is out (a drag, Revert, the offer
    // put-back): the value would land in the job the answer replaces
    // (round 12, fix design 1).
    if (anyWriterOpen()) return;
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
      const switched = next.key !== opts.key;
      opts = next;
      if (switched) {
        // Another item: what was sent or stepped for the one before is not
        // this item's (the same value typed, one step), nor its refusal.
        item++;
        out = null;
        stepped = null;
        refusal = null;
        if (invalid) setInvalid(null);
      } else if (!dirty && invalid && !stays && fmt(opts.value) !== input.value) setInvalid(null);
      render();
      syncDisabled();
    },
    destroy() {
      window.removeEventListener("pointerdown", onOutsidePress, true);
      clearTimeout(clearTimer);
      clearTimeout(stepTimer);
      stop();
      observer.disconnect();
      resize?.disconnect();
      spin?.remove();
    },
  };
}
