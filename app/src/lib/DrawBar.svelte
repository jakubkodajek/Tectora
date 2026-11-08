<script lang="ts">
  // Footprint entry like DACAD's point-entry bar: type a wall length and
  // press an arrow (button or keyboard) to add the next corner.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { notifyError } from "./notify.svelte";
  import { numeric } from "./numeric.svelte";
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
    oncancel: () => void;
  } = $props();

  let length = $state("5,00");

  const DIRS: { key: string; dx: number; dy: number; rot: number }[] = [
    { key: "ArrowLeft", dx: -1, dy: 0, rot: 180 },
    { key: "ArrowUp", dx: 0, dy: 1, rot: -90 },
    { key: "ArrowRight", dx: 1, dy: 0, rot: 0 },
    { key: "ArrowDown", dx: 0, dy: -1, rot: 90 },
    { key: "", dx: -Math.SQRT1_2, dy: Math.SQRT1_2, rot: -135 },
    { key: "", dx: Math.SQRT1_2, dy: Math.SQRT1_2, rot: -45 },
    { key: "", dx: -Math.SQRT1_2, dy: -Math.SQRT1_2, rot: 135 },
    { key: "", dx: Math.SQRT1_2, dy: -Math.SQRT1_2, rot: 45 },
  ];

  function go(dx: number, dy: number) {
    const l = parseFloat(length.replace(",", "."));
    if (!Number.isFinite(l) || l <= 0) return;
    onstep(+(dx * l).toFixed(3), +(dy * l).toFixed(3));
  }

  function onkeydown(e: KeyboardEvent) {
    const d = DIRS.find((d) => d.key && d.key === e.key);
    // The preset fields keep their own keys (Enter stores, arrows step).
    if ((e.target as HTMLElement).classList?.contains("num-input")) return;
    if (d && count > 0) {
      e.preventDefault();
      go(d.dx, d.dy);
    } else if (e.key === "Enter" && count >= 3) {
      e.preventDefault();
      onclose();
    } else if (e.key === "Backspace" && (e.target as HTMLElement).tagName !== "INPUT") {
      e.preventDefault();
      onundo();
    } else if (e.key === "Escape") {
      oncancel();
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
    <div class="unit"><input type="text" inputmode="decimal" bind:value={length} /><em>m</em></div>
  </label>
  <div class="arrows" aria-label={t("draw.direction")}>
    {#each DIRS as d}
      <button class="icon-btn" disabled={count === 0} onclick={() => go(d.dx, d.dy)} title={t("draw.direction")}>
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
    <button class="btn sm" onclick={oncancel}>{t("draw.cancel")}</button>
    <button class="btn sm primary" disabled={count < 3} onclick={onclose}><Icon name="check" size={15} />{t("draw.close")}</button>
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
  .btn.sm {
    height: 32px;
    padding: 0 12px;
    font-size: 13px;
    white-space: nowrap;
  }
</style>
