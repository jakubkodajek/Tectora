<script lang="ts">
  // DACAD's "Standard-Dach": pick a basic roof type for a rectangle and
  // enter its dimensions; shows a live plan preview with roof lines.
  import { api } from "./api";
  import PlanThumb from "./PlanThumb.svelte";
  import { m2 } from "./format";
  import { t } from "./i18n.svelte";
  import { presets } from "./settings.svelte";
  import type { RoofPart, StandardKind } from "./types";
  import { STANDARD_KINDS, emptyProject } from "./types";

  let { part = $bindable(null) }: { part: RoofPart | null } = $props();

  let kind = $state<StandardKind>("gable");
  let length = $state(12);
  let width = $state(8);
  let pitch = $state(presets.pitch_deg);
  let eave = $state(presets.eave_height);
  let overhang = $state(presets.overhang);
  let preview = $state<{ outline: [number, number][]; lines: [[number, number], [number, number]][]; area: number } | null>(null);
  let seq = 0;

  // Simple elevation sketches of each type (viewBox 0 0 64 40).
  const ICONS: Record<StandardKind, string> = {
    gable: "M6 34V20L32 6l26 14v14z M6 20h52",
    hip: "M6 34V22l14-14h24l14 14v12z M6 22h52",
    half_hip: "M6 34V20l20-12h12l20 12v14z M26 8l-6 12M38 8l6 12M6 20h52",
    shed: "M6 34V14l52-8v28z M6 14l52-8",
    pyramid: "M10 34V22l22-16 22 16v12z M10 22h44M32 6v16",
  };

  $effect(() => {
    const params = { kind, length, width, pitchDeg: pitch, eaveHeight: eave, overhang };
    const ok = length > 0 && width > 0 && pitch > 0 && pitch < 90;
    const my = ++seq;
    if (!ok) {
      part = null;
      return;
    }
    api.standardPart(params).then(async (p) => {
      if (my !== seq) return;
      part = p;
      const project = emptyProject("preview");
      project.parts = [p];
      const r = await api.calc(project);
      if (my !== seq) return;
      preview = {
        outline: p.edges.map((e) => [e.start.x, e.start.y]),
        lines: r.parts[0].geometry.lines.filter((l) => l.kind !== "eave").map((l) => [[l.a[0], l.a[1]], [l.b[0], l.b[1]]]),
        area: r.totals.roof_area,
      };
    });
  });
</script>

<div class="form">
  <div class="types" role="radiogroup" aria-label={t("std.type")}>
    {#each STANDARD_KINDS as k}
      <button class="type" class:on={kind === k} role="radio" aria-checked={kind === k} onclick={() => (kind = k)}>
        <svg viewBox="0 0 64 40" aria-hidden="true"><path d={ICONS[k]} /></svg>
        <span>{t(`std.${k}`)}</span>
      </button>
    {/each}
  </div>
  <div class="row">
    <div class="fields">
      <label><span>{t("std.length")}</span><div class="unit"><input type="number" step="0.1" min="1" bind:value={length} /><em>m</em></div></label>
      <label class:off={kind === "pyramid"}><span>{t("std.width")}</span><div class="unit"><input type="number" step="0.1" min="1" disabled={kind === "pyramid"} bind:value={width} /><em>m</em></div></label>
      <label><span>{t("edge.pitch")}</span><div class="unit"><input type="number" step="1" min="1" max="89" bind:value={pitch} /><em>°</em></div></label>
      <label><span>{t("edge.eaveHeight")}</span><div class="unit"><input type="number" step="0.05" bind:value={eave} /><em>m</em></div></label>
      <label><span>{t("edge.overhang")}</span><div class="unit"><input type="number" step="0.05" min="0" bind:value={overhang} /><em>m</em></div></label>
    </div>
    <div class="preview">
      {#if preview}
        <PlanThumb outlines={[preview.outline]} lines={preview.lines} padding={0.1} />
        <span class="area">{m2(preview.area)}</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .types {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 8px;
  }
  .type {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 10px 6px 8px;
    border: 1.5px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-2);
  }
  .type svg {
    width: 56px;
    height: 36px;
  }
  .type path {
    fill: var(--plan-fill);
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linejoin: round;
  }
  .type:hover {
    border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
  }
  .type.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 200px;
    gap: 16px;
  }
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 12px;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  label.off {
    opacity: 0.4;
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 100%;
    padding-right: 26px;
  }
  .unit em {
    position: absolute;
    right: 10px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
  }
  .preview {
    position: relative;
    height: 190px;
    padding: 12px;
    border-radius: 10px;
    background:
      linear-gradient(var(--plan-grid) 1px, transparent 1px) 0 0 / 14px 14px,
      linear-gradient(90deg, var(--plan-grid) 1px, transparent 1px) 0 0 / 14px 14px,
      var(--surface-2);
  }
  .area {
    position: absolute;
    right: 8px;
    bottom: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
</style>
