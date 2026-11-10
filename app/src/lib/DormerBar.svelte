<script lang="ts">
  // DACAD's dormer definition (Gaube definieren): type, width, position,
  // eave height and pitches of the selected dormer, docked like the wall bar.
  import Icon from "./Icon.svelte";
  import { numberLocale, t } from "./i18n.svelte";
  import type { CalcResult, Dormer, Project } from "./types";
  import { materialIds, materialName } from "./materials";
  import { DORMER_KINDS, buildDormer, defaultParams, parseDormer, roofHeightAt, type DormerKind, type DormerParams } from "./dormers";
  import { MAX_ROUND_SEGMENTS } from "./roundroof";
  import { formatNumber, numeric } from "./numeric.svelte";

  let {
    project,
    result,
    index = $bindable(),
    onchange,
    ondelete,
    onedit,
  }: {
    project: Project;
    result: CalcResult | null;
    index: number | null;
    onchange: (d: Dormer) => void;
    ondelete: () => void;
    /** Opens the dormer window (plan and section). */
    onedit?: () => void;
  } = $props();

  const dormer = $derived(index !== null ? project.dormers[index] : null);
  const params = $derived(dormer ? parseDormer(dormer) : null);
  /** Host roof height at the middle of the front, for "on the roof" eaves. */
  const host = $derived.by(() => {
    if (!params) return null;
    const m = {
      x: params.start.x + (Math.cos(params.angle) * params.width) / 2,
      y: params.start.y + (Math.sin(params.angle) * params.width) / 2,
    };
    // Just inside the front, where the host face is.
    const n = { x: -Math.sin(params.angle) * 0.01, y: Math.cos(params.angle) * 0.01 };
    return roofHeightAt(result, { x: m.x + n.x, y: m.y + n.y });
  });
  const kind = $derived<DormerKind | null>(
    params && params.kind === "gable" && host !== null && Math.abs(params.eave_height - host) < 0.01 ? "pointed" : (params?.kind ?? null),
  );
  const angleDeg = $derived(params ? +(((params.angle * 180) / Math.PI + 360) % 360).toFixed(2) : 0);


  function set(patch: Partial<DormerParams>) {
    if (!params) return;
    const next = { ...params, ...patch };
    if (next.kind === "general") return;
    // A front pitch carries over only between kinds that have one.
    const fronted = (k: DormerKind) => k === "shed" || k === "trapezoid";
    if (patch.kind && fronted(patch.kind) && !fronted(params.kind)) {
      next.front_pitch_deg = defaultParams(patch.kind, next.start, next.angle, next.eave_height).front_pitch_deg;
    }
    if (next.kind === "pointed" && host !== null && patch.kind) next.eave_height = +host.toFixed(3);
    onchange(buildDormer(next));
  }

  function step(d: number) {
    const n = project.dormers.length;
    if (n === 0) return;
    index = ((index ?? -1) + d + n) % n;
  }
</script>

<div class="bar" role="toolbar" aria-label={t("dormer.title")}>
  <div class="nav">
    <button class="icon-btn" onclick={() => step(-1)} title={t("dormer.prev")} aria-label={t("dormer.prev")}><Icon name="left" /></button>
    <div class="which">
      <strong>{t("dormer.n", { n: (index ?? 0) + 1 })}</strong>
      <span>{t("def.of", { n: project.dormers.length })}</span>
    </div>
    <button class="icon-btn" onclick={() => step(1)} title={t("dormer.next")} aria-label={t("dormer.next")}><Icon name="right" /></button>
  </div>

  {#if params && kind}
    {#if kind === "general"}
      <p class="tip">{t("dormer.general")}</p>
    {:else}
      <label>
        <span>{t("dormer.kind")}</span>
        <select value={kind} onchange={(e) => set({ kind: (e.target as HTMLSelectElement).value as DormerKind })}>
          {#each DORMER_KINDS as k}
            <option value={k}>{t(`dormer.${k}`)}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>{t("dormer.width")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.width, step: 0.05, min: 0.1, onchange: (v) => set({ width: v }) }} /><em>m</em></div>
      </label>
      <label>
        <span>X</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.start.x, step: 0.05, onchange: (v) => set({ start: { ...params.start, x: v } }) }} /><em>m</em></div>
      </label>
      <label>
        <span>Y</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.start.y, step: 0.05, onchange: (v) => set({ start: { ...params.start, y: v } }) }} /><em>m</em></div>
      </label>
      <label>
        <span>{t("edge.direction")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: angleDeg, step: 1, onchange: (v) => set({ angle: (v * Math.PI) / 180 }) }} /><em>°</em></div>
      </label>
      <label title={host !== null ? t("dormer.hostHint", { h: formatNumber(host, numberLocale(), 2) }) : undefined}>
        <span>{t("edge.eaveHeight")}</span>
        <div class="unit"><input type="text" disabled={kind === "pointed"} use:numeric={{ value: params.eave_height, step: 0.05, onchange: (v) => set({ eave_height: v }) }} /><em>m</em></div>
      </label>
      <label title={t("edge.materialHint")}>
        <span>{t("edge.material")}</span>
        <select value={params.material} onchange={(e) => set({ material: +(e.target as HTMLSelectElement).value })}>
          {#each materialIds(project) as m}
            <option value={m}>{materialName(project, m)}</option>
          {/each}
        </select>
      </label>
      {#if kind === "round"}
        <label>
          <span>{t("round.segments")}</span>
          <input type="text" use:numeric={{ value: params.segments, step: 1, min: 1, max: MAX_ROUND_SEGMENTS, onchange: (v) => set({ segments: Math.round(v) }) }} />
        </label>
      {/if}
      {#if kind !== "shed" && kind !== "round"}
        <label>
          <span>{kind === "trapezoid" ? t("dormer.cheekPitch") : t("edge.pitch")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: params.pitch_deg, step: 1, min: 0.1, max: 89.9, onchange: (v) => set({ pitch_deg: v }) }} /><em>°</em></div>
        </label>
      {/if}
      {#if kind === "shed" || kind === "trapezoid"}
        <label>
          <span>{t("dormer.frontPitch")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: params.front_pitch_deg, step: 1, min: 0.1, max: 89.9, onchange: (v) => set({ front_pitch_deg: v }) }} /><em>°</em></div>
        </label>
      {/if}
      {#if kind === "half_hip"}
        <label>
          <span>{t("dormer.halfHipHeight")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: params.half_hip_height, step: 0.05, min: 0.05, onchange: (v) => set({ half_hip_height: v }) }} /><em>m</em></div>
        </label>
        <label>
          <span>{t("edge.halfHipPitch")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: params.half_hip_pitch_deg, step: 1, min: 0.1, max: 89.9, onchange: (v) => set({ half_hip_pitch_deg: v }) }} /><em>°</em></div>
        </label>
      {/if}
      <label>
        <span>{t("dormer.frontOverhang")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.overhang, step: 0.05, min: 0, onchange: (v) => set({ overhang: v }) }} /><em>m</em></div>
      </label>
      <label>
        <span>{t("dormer.sideOverhang")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.side_overhang, step: 0.05, min: 0, onchange: (v) => set({ side_overhang: v }) }} /><em>m</em></div>
      </label>
    {/if}
    <div class="spacer"></div>
    {#if onedit && kind !== "general"}
      <button class="btn" onclick={onedit} title={t("dormer.editHint")}>{t("dormer.edit")}</button>
    {/if}
    <button class="icon-btn danger" onclick={ondelete} title={t("dormer.delete")} aria-label={t("dormer.delete")}><Icon name="trash" /></button>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
    padding: 10px 14px 12px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    min-height: 70px;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 14px;
    border-right: 1px solid var(--border);
    align-self: stretch;
  }
  .which {
    display: flex;
    flex-direction: column;
    min-width: 92px;
    line-height: 1.25;
  }
  .which strong {
    font-size: 14px;
  }
  .which span {
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
  }
  select {
    height: 34px;
    min-width: 150px;
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 92px;
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
  .spacer {
    flex: 1;
  }
  .danger {
    color: var(--danger);
  }
  .tip {
    align-self: center;
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
</style>
