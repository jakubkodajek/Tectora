<script lang="ts">
  // Footprint entry like DACAD's point-entry bar: type a wall length and
  // press an arrow (button or keyboard) to add the next corner.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { notifyError } from "./notify.svelte";
  import { EAVE_HEIGHT, OVERHANG } from "./limits";
  import { numeric, parseNumber } from "./numeric.svelte";
  import { presets, savePresets, updateSettings } from "./settings.svelte";
  import { MAX_ANGLE, angledStep, type Step } from "./drawstep";

  let {
    count,
    prev = null,
    onstep,
    onundo,
    onclose,
    oncancel,
  }: {
    count: number;
    /** The last wall drawn, for an angle measured from it. */
    prev?: Step | null;
    onstep: (dx: number, dy: number) => void;
    onundo: () => void;
    onclose: () => void;
    /** Stops drawing; `ask`: a key did it, so ask first when corners would be lost. */
    oncancel: (ask?: boolean) => void;
  } = $props();

  /** The wall length, m: a number field like every other (comma or dot, spaces). */
  let length = $state(5);
  /** Longest wall the field takes, m [no source: far beyond any building, inside the core's ±100 km]. */
  const MAX_LENGTH = 10000;
  let lengthInput = $state<HTMLInputElement>();
  /** The typed direction, degrees (0° = right, 90° = up, anticlockwise), or the turn from the last wall (review UX-007). */
  let angle = $state(0);
  let angleInput = $state<HTMLInputElement>();
  /** The angle is a turn from the previous wall instead of a direction on the plan. */
  let relative = $state(false);

  const DIRS: { key: string; dx: number; dy: number; rot: number; name: string }[] = [
    { key: "ArrowLeft", dx: -1, dy: 0, rot: 180, name: "draw.dirLeft" },
    { key: "ArrowUp", dx: 0, dy: 1, rot: -90, name: "draw.dirUp" },
    { key: "ArrowRight", dx: 1, dy: 0, rot: 0, name: "draw.dirRight" },
    { key: "ArrowDown", dx: 0, dy: -1, rot: 90, name: "draw.dirDown" },
    { key: "", dx: -Math.SQRT1_2, dy: Math.SQRT1_2, rot: -135, name: "draw.dirUpLeft" },
    { key: "", dx: Math.SQRT1_2, dy: Math.SQRT1_2, rot: -45, name: "draw.dirUpRight" },
    { key: "", dx: -Math.SQRT1_2, dy: -Math.SQRT1_2, rot: 135, name: "draw.dirDownLeft" },
    { key: "", dx: Math.SQRT1_2, dy: -Math.SQRT1_2, rot: 45, name: "draw.dirDownRight" },
  ];

  /** The length to draw: the typed text when it is a valid length, else null (the field says why). */
  function typedLength(): number | null {
    if (!lengthInput) return length;
    // A refused entry stays marked for a moment after the field restored
    // it: drawing the restored length then would add a wall nobody typed.
    if (lengthInput.getAttribute("aria-invalid") === "true") return null;
    const v = parseNumber(lengthInput.value);
    return v !== null && v >= 0.01 && v <= MAX_LENGTH ? v : null;
  }

  function go(dx: number, dy: number) {
    const l = typedLength();
    if (l === null) return;
    length = l;
    onstep(+(dx * l).toFixed(3), +(dy * l).toFixed(3));
  }

  /** The angle to draw: the typed text when valid, else null (the field says why). */
  function typedAngle(): number | null {
    if (!angleInput) return angle;
    if (angleInput.getAttribute("aria-invalid") === "true") return null;
    const v = parseNumber(angleInput.value);
    return v !== null && Math.abs(v) <= MAX_ANGLE ? v : null;
  }

  /** Adds a wall of the typed length in the typed angle (Enter in the angle field, or its button). */
  function goAngle() {
    const l = typedLength();
    const a = typedAngle();
    if (l === null || a === null || count === 0) return;
    const s = angledStep(l, a, relative ? prev : null);
    if (!s) return;
    length = l;
    angle = a;
    onstep(s.dx, s.dy);
  }

  function onkeydown(e: KeyboardEvent) {
    const d = DIRS.find((d) => d.key && d.key === e.key);
    const inLength = !!lengthInput && e.target === lengthInput;
    if (angleInput && e.target === angleInput) {
      // The angle field's own number keys stay (arrows step it); Enter adds
      // the wall in that angle, Escape only undoes the typing.
      if (e.key === "Enter") {
        e.preventDefault();
        goAngle();
      } else if (e.key === "Escape") {
        e.preventDefault();
        angleInput.blur();
      }
      return;
    }
    // The preset fields keep their own keys (Enter stores, arrows step).
    if (!inLength && (e.target as HTMLElement).classList?.contains("num-input")) return;
    if (inLength && e.key === "Escape") {
      // Escape in the length field only undoes the typing (the field
      // restores the last length); it never throws the footprint away.
      e.preventDefault();
      lengthInput!.blur();
      return;
    }
    if (d && count > 0) {
      e.preventDefault();
      go(d.dx, d.dy);
    } else if ((e.key === "Enter" || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "i")) && count >= 3) {
      e.preventDefault();
      onclose();
    } else if (e.key === "Backspace" && (e.target as HTMLElement).tagName !== "INPUT") {
      e.preventDefault();
      onundo();
    } else if (e.key === "Escape") {
      oncancel(true);
    }
  }

  function preset(v: number, key: "pitch_deg" | "eave_height" | "overhang") {
    presets[key] = v;
    savePresets();
    // The same defaults as in Settings: store them in the settings file,
    // or the next settings change (or start) puts the old ones back.
    updateSettings({ roof_defaults: { [key]: v } }).catch((e) => notifyError(e, "save"));
  }
</script>

<svelte:window {onkeydown} />

<div class="bar" role="toolbar" aria-label={t("draw.title")}>
  <div class="title">
    <strong>{t("draw.title")}</strong>
    <span>{count === 0 ? t("draw.start") : t("draw.points", { n: count })}</span>
  </div>
  <label>
    <span>{t("draw.length")}</span>
    <div class="unit"><input type="text" bind:this={lengthInput} use:numeric={{ value: length, digits: 2, step: 0.5, min: 0.01, max: MAX_LENGTH, spinner: false, arrows: false, onchange: (v) => (length = v) }} /><em>m</em></div>
  </label>
  <div class="angle">
    <label title={t("draw.angleHint")}>
      <span>{t("draw.angle")}</span>
      <div class="unit"><input type="text" bind:this={angleInput} data-testid="draw-angle" use:numeric={{ value: angle, step: 15, min: -MAX_ANGLE, max: MAX_ANGLE, onchange: (v) => (angle = v) }} /><em>°</em></div>
    </label>
    <button class="icon-btn go" data-testid="draw-angle-go" disabled={count === 0} onclick={goAngle} title={t("draw.angleGo")} aria-label={t("draw.angleGo")}>
      <svg width="16" height="16" viewBox="0 0 24 24" style="transform:rotate({-angle}deg)" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h15M13 6l6 6-6 6" /></svg>
    </button>
    <label class="check" title={t("draw.relativeHint")}>
      <input type="checkbox" bind:checked={relative} data-testid="draw-relative" />{t("draw.relative")}
    </label>
  </div>
  <div class="arrows" role="group" aria-label={t("draw.direction")}>
    {#each DIRS as d}
      <button class="icon-btn" disabled={count === 0} onclick={() => go(d.dx, d.dy)} title={t(d.name)} aria-label={t(d.name)}>
        <svg width="16" height="16" viewBox="0 0 24 24" style="transform:rotate({d.rot}deg)" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h15M13 6l6 6-6 6" /></svg>
      </button>
    {/each}
  </div>
  <!-- App-wide defaults, said as such: a value typed here applies to every
       later building and job (Settings › New roofs), not only this one (UX-002). -->
  <div class="presets" role="group" aria-labelledby="draw-presets-lbl" aria-describedby="draw-presets-note" title={t("draw.presetsHint")}>
    <span class="lbl" id="draw-presets-lbl">{t("draw.presets")}<small id="draw-presets-note" data-testid="draw-presets-note">{t("draw.presetsNote")}</small></span>
    <label><span>{t("edge.pitch")}</span><div class="unit"><input type="text" use:numeric={{ value: presets.pitch_deg, step: 1, min: 1, max: 89, onchange: (v) => preset(v, "pitch_deg") }} /><em>°</em></div></label>
    <label title={t("edge.eaveHeightPresetHint")}><span>{t("edge.eaveHeight")}</span><div class="unit"><input type="text" use:numeric={{ value: presets.eave_height, step: 0.05, min: EAVE_HEIGHT.min, max: EAVE_HEIGHT.max, onchange: (v) => preset(v, "eave_height") }} /><em>m</em></div></label>
    <label><span>{t("edge.overhang")}</span><div class="unit"><input type="text" use:numeric={{ value: presets.overhang, step: 0.05, min: OVERHANG.min, max: OVERHANG.max, onchange: (v) => preset(v, "overhang") }} /><em>m</em></div></label>
  </div>
  <div class="actions">
    <button class="btn sm undo" disabled={count === 0} onclick={onundo} aria-label={t("draw.undo")} title={t("draw.undo")}><Icon name="undo" size={15} /><span class="txt">{t("draw.undo")}</span></button>
    <button class="btn sm" onclick={() => oncancel(false)}>{t("draw.cancel")}</button>
    <button class="btn sm primary" data-testid="draw-close" disabled={count < 3} onclick={onclose} title={t("draw.closeHint")}><Icon name="check" size={15} /><span class="txt">{t("draw.close")}</span></button>
  </div>
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
    padding: 10px 14px 12px;
    background: color-mix(in srgb, var(--accent) 5%, var(--surface));
    border-top: 2px solid var(--accent);
    min-height: 70px;
    overflow-x: auto;
  }
  .title {
    display: flex;
    flex-direction: column;
    align-self: center;
    min-width: 110px;
    max-width: 150px;
    line-height: 1.3;
  }
  .title span {
    font-size: 12px;
    color: var(--text-2);
  }
  label span,
  .lbl {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 84px;
    padding-right: 24px;
  }
  .unit em {
    position: absolute;
    right: 9px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
  }
  .angle {
    display: flex;
    align-items: flex-end;
    gap: 6px;
  }
  .angle .unit input {
    width: 76px;
  }
  .angle .go {
    width: 30px;
    height: 32px;
    border: 1px solid var(--border);
    background: var(--surface);
  }
  .angle .check {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 32px;
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .arrows {
    display: grid;
    grid-template-columns: repeat(4, 30px);
    gap: 2px;
  }
  .arrows .icon-btn {
    width: 30px;
    height: 30px;
    border: 1px solid var(--border);
    background: var(--surface);
  }
  .presets {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding-left: 14px;
    border-left: 1px solid var(--border);
  }
  .presets .lbl {
    align-self: center;
    margin: 0;
    font-weight: 600;
  }
  .presets .lbl small {
    display: block;
    max-width: 130px;
    margin-top: 2px;
    font-weight: 400;
    font-size: 11px;
    line-height: 1.2;
    white-space: normal;
    color: var(--text-2);
  }
  .presets .unit input {
    width: 76px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
  /* A low window: the finish button stays in sight, the rest scrolls. */
  @media (max-height: 560px) {
    .bar {
      max-height: 42vh;
      overflow-y: auto;
    }
    .actions {
      order: -1;
      flex-basis: 100%;
      justify-content: flex-end;
    }
  }
  .btn.sm {
    height: 32px;
    padding: 0 12px;
    font-size: 13px;
    white-space: nowrap;
  }
  .actions .btn.primary {
    min-width: 0;
  }
  .btn .txt {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* A narrow window: "Remove last point" is its icon (its name stays the
     button's label and tooltip), so the German / Slovak finish button fits
     (sweep 2026-11-15). */
  @media (max-width: 760px) {
    .undo .txt {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0 0 0 0);
    }
    .undo {
      padding: 0 9px;
    }
  }
</style>
