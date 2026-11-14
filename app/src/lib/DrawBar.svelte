<script lang="ts">
  // Footprint entry like DACAD's point-entry bar: type a wall length and
  // press an arrow (button or keyboard) to add the next corner.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { notifyError } from "./notify.svelte";
  import { numeric, parseNumber } from "./numeric.svelte";
  import { presets, savePresets, updateSettings } from "./settings.svelte";

  let {
    count,
    onstep,
    onundo,
    onclose,
    oncancel,
  }: {
    count: number;
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

  function onkeydown(e: KeyboardEvent) {
    const d = DIRS.find((d) => d.key && d.key === e.key);
    const inLength = !!lengthInput && e.target === lengthInput;
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
  <div class="arrows" role="group" aria-label={t("draw.direction")}>
    {#each DIRS as d}
      <button class="icon-btn" disabled={count === 0} onclick={() => go(d.dx, d.dy)} title={t(d.name)} aria-label={t(d.name)}>
        <svg width="16" height="16" viewBox="0 0 24 24" style="transform:rotate({d.rot}deg)" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h15M13 6l6 6-6 6" /></svg>
      </button>
    {/each}
  </div>
  <div class="presets">
    <span class="lbl">{t("draw.presets")}</span>
    <label><span>{t("edge.pitch")}</span><div class="unit"><input type="text" use:numeric={{ value: presets.pitch_deg, step: 1, min: 1, max: 89, onchange: (v) => preset(v, "pitch_deg") }} /><em>°</em></div></label>
    <label><span>{t("edge.eaveHeight")}</span><div class="unit"><input type="text" use:numeric={{ value: presets.eave_height, step: 0.05, min: -100, max: 1000, onchange: (v) => preset(v, "eave_height") }} /><em>m</em></div></label>
    <label><span>{t("edge.overhang")}</span><div class="unit"><input type="text" use:numeric={{ value: presets.overhang, step: 0.05, min: 0, max: 20, onchange: (v) => preset(v, "overhang") }} /><em>m</em></div></label>
  </div>
  <div class="actions">
    <button class="btn sm" disabled={count === 0} onclick={onundo}><Icon name="undo" size={15} />{t("draw.undo")}</button>
    <button class="btn sm" onclick={() => oncancel(false)}>{t("draw.cancel")}</button>
    <button class="btn sm primary" data-testid="draw-close" disabled={count < 3} onclick={onclose} title={t("draw.closeHint")}><Icon name="check" size={15} />{t("draw.close")}</button>
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
</style>
