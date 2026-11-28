<script lang="ts">
  // The tower dialog: a preset profile to start from, the shaft's plan
  // shape and the roof's tiers, with a section sketch and the areas from
  // the core (`tower_preview`). OK stores it through `set_tower` (the MCP
  // tool of the same name); nothing of the roof is calculated here.
  import { api } from "./api";
  import { refusalReason, toAppError } from "./errors";
  import { m, m2 } from "./format";
  import { numberLocale, t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";
  import { materialIds, materialName } from "./materials";
  import { numeric } from "./numeric.svelte";
  import { towerSection } from "./towers";
  import { maxOf } from "./extent";
  import { TOWER_PRESETS, type Project, type Tower, type TowerPresetKind, type TowerPreview, type TowerShape } from "./types";

  let {
    project,
    tower = $bindable(),
    blocked = $bindable(null),
  }: {
    project: Project;
    tower: Tower;
    /** Why OK cannot store the tower now (the core refused it), else null. */
    blocked?: string | null;
  } = $props();

  /** At most this many tiers (core `tower::MAX_TIERS`). */
  const MAX_TIERS = 24;
  const SHAPES: TowerShape[] = ["rectangle", "polygon", "circle"];

  let preview = $state<TowerPreview | null>(null);
  /** Bumped when rows are added, removed or moved, so the tier fields are made anew for their rows. */
  let rev = $state(0);
  let seq = 0;
  let presetBusy = $state(false);

  $effect(() => {
    const snap = $state.snapshot(tower) as Tower;
    const mine = ++seq;
    api
      .towerPreview(snap)
      .then((p) => {
        if (mine !== seq) return;
        preview = p;
        blocked = null;
      })
      .catch((e) => {
        if (mine !== seq) return;
        preview = null;
        const err = toAppError(e);
        blocked = err.code === "refused" ? refusalReason(err.detail) : t("refuse.other");
      });
  });

  function set(patch: Partial<Tower>) {
    tower = { ...tower, ...patch };
  }
  function setTier(i: number, patch: Partial<Tower["tiers"][number]>) {
    tower.tiers[i] = { ...tower.tiers[i], ...patch };
  }
  function addTier() {
    if (tower.tiers.length >= MAX_TIERS) return;
    const last = tower.tiers[tower.tiers.length - 1];
    // The new tier goes on top: a point of a third of the width.
    tower.tiers.push({ rise: +(tower.width / 3).toFixed(2), top_width: 0, material: last?.material ?? 2 });
    if (last && last.top_width === 0) last.top_width = +(tower.width / 3).toFixed(2);
    rev++;
  }
  function removeTier(i: number) {
    tower.tiers.splice(i, 1);
    rev++;
  }
  function moveTier(i: number, d: -1 | 1) {
    const j = i + d;
    if (j < 0 || j >= tower.tiers.length) return;
    const [x] = tower.tiers.splice(i, 1);
    tower.tiers.splice(j, 0, x);
    rev++;
  }

  async function usePreset(kind: TowerPresetKind) {
    if (presetBusy) return;
    presetBusy = true;
    try {
      const p = await api.towerPreset(kind, { ...tower.center }, tower.width, tower.base_height);
      tower = { ...p, label: tower.label, rotation_deg: tower.rotation_deg };
      rev++;
    } catch (e) {
      const err = toAppError(e);
      blocked = refusalReason(err.detail);
    } finally {
      presetBusy = false;
    }
  }

  const deg = (v: number) => `${new Intl.NumberFormat(numberLocale(), { maximumFractionDigits: 1 }).format(v)}°`;

  /** The section sketch: the profile's half-widths over the heights, mirrored, with the shaft below. */
  const sketch = $derived.by(() => {
    const pts = towerSection(tower, preview?.result ?? null);
    if (pts.length < 2) return null;
    const W = 200;
    const H = 220;
    const pad = 12;
    const base = pts[0].z;
    const shaft = Math.max(0.5, (pts[pts.length - 1].z - base) * 0.2);
    const zMin = base - shaft;
    const zMax = maxOf(pts, (p) => p.z);
    const half = Math.max(maxOf(pts, (p) => p.half), 0.01);
    const k = Math.min((W / 2 - pad) / half, (H - 2 * pad) / Math.max(zMax - zMin, 0.01));
    const X = (x: number) => W / 2 + x * k;
    const Y = (z: number) => H - pad - (z - zMin) * k;
    const right = pts.map((p) => `${X(p.half).toFixed(1)},${Y(p.z).toFixed(1)}`);
    const left = [...pts].reverse().map((p) => `${X(-p.half).toFixed(1)},${Y(p.z).toFixed(1)}`);
    const b = pts[0].half;
    // Tier boundaries: a level line at every tier's top.
    const levels = (preview?.result.tiers ?? []).map((x) => {
      const top = pts.find((p) => Math.abs(p.z - x.top_height) < 1e-6);
      const h = top?.half ?? 0;
      return h > 1e-6 ? `M${X(-h).toFixed(1)},${Y(x.top_height).toFixed(1)}H${X(h).toFixed(1)}` : "";
    });
    return {
      W,
      H,
      roof: `M${right.join("L")}L${left.join("L")}`,
      shaft: `M${X(-b).toFixed(1)},${Y(zMin).toFixed(1)}V${Y(base).toFixed(1)}M${X(b).toFixed(1)},${Y(zMin).toFixed(1)}V${Y(base).toFixed(1)}`,
      levels: levels.join(""),
      axis: `M${X(0)},${pad}V${H - pad}`,
    };
  });
  const shape = $derived(tower.shape ?? "rectangle");
</script>

<div class="dlg">
  <div class="presets" role="group" aria-label={t("tower.presets")}>
    {#each TOWER_PRESETS as k}
      <button class="preset" disabled={presetBusy} onclick={() => usePreset(k)} data-testid="tower-preset-{k}">
        {t(`tower.preset.${k}`)}
      </button>
    {/each}
  </div>
  <p class="hint">{t("tower.presetHint")}</p>

  <div class="body">
    <div class="fields">
      <label class="wide"><span>{t("tower.label")}</span>
        <input type="text" value={tower.label ?? ""} maxlength="200" oninput={(e) => set({ label: (e.target as HTMLInputElement).value })} />
      </label>
      <label><span>{t("tower.shape")}</span>
        <select value={shape} onchange={(e) => set({ shape: (e.target as HTMLSelectElement).value as TowerShape })}>
          {#each SHAPES as s}<option value={s}>{t(`tower.shape.${s}`)}</option>{/each}
        </select>
      </label>
      <label><span>{t("tower.width")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: tower.width, step: 0.1, min: 0.1, max: 100, live: true, onchange: (v) => set({ width: v }) }} /><em>m</em></div>
      </label>
      {#if shape === "rectangle"}
        <label><span>{t("tower.depth")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: tower.depth ?? tower.width, step: 0.1, min: 0.1, max: 100, live: true, onchange: (v) => set({ depth: v }) }} /><em>m</em></div>
        </label>
      {/if}
      {#if shape === "polygon"}
        <label><span>{t("tower.sides")}</span>
          <input type="text" use:numeric={{ value: tower.sides ?? 8, step: 1, min: 3, max: 64, live: true, onchange: (v) => set({ sides: Math.round(v) }) }} />
        </label>
      {/if}
      {#if shape !== "circle"}
        <label><span>{t("tower.cornerRadius")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: tower.corner_radius ?? 0, step: 0.05, min: 0, max: 50, live: true, onchange: (v) => set({ corner_radius: v }) }} /><em>m</em></div>
        </label>
      {/if}
      <label><span>{t("tower.rotation")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: tower.rotation_deg ?? 0, step: 5, min: -360, max: 360, live: true, onchange: (v) => set({ rotation_deg: v }) }} /><em>°</em></div>
      </label>
      <label class="wide"><span>{t("tower.baseHeight")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: tower.base_height, step: 0.1, min: 0, max: 1000, live: true, onchange: (v) => set({ base_height: v }) }} /><em>m</em></div>
      </label>
    </div>

    <figure class="view">
      <figcaption>{t("tower.preview")}</figcaption>
      {#if sketch}
        <svg viewBox="0 0 {sketch.W} {sketch.H}" aria-hidden="true">
          <path class="axis" d={sketch.axis} />
          <path class="shaft" d={sketch.shaft} />
          <path class="levels" d={sketch.levels} />
          <path class="roof" d={sketch.roof} />
        </svg>
      {:else}
        <div class="empty"></div>
      {/if}
      {#if preview}
        <dl class="facts" data-testid="tower-facts">
          <div><dt>{t("tower.roofArea")}</dt><dd>{m2(preview.roof_area)}</dd></div>
          {#if preview.wall_area >= 0.005}<div><dt>{t("tower.wallArea")}</dt><dd>{m2(preview.wall_area)}</dd></div>{/if}
          <div><dt>{t("tower.topHeight")}</dt><dd>{m(preview.result.top_height)}</dd></div>
        </dl>
      {/if}
    </figure>
  </div>

  <div class="tiers-head">
    <strong>{t("tower.tiers")}</strong>
    <button class="btn sm" disabled={tower.tiers.length >= MAX_TIERS} onclick={addTier}><Icon name="plus" size={15} />{t("tower.addTier")}</button>
  </div>
  <p class="hint">{t("tower.bulgeHint")}</p>
  {#key rev}
    <div class="tiers" role="table" aria-label={t("tower.tiers")}>
      <div class="tr th" role="row">
        <span role="columnheader">#</span>
        <span role="columnheader">{t("tower.rise")}</span>
        <span role="columnheader">{t("tower.topWidth")}</span>
        <span role="columnheader">{t("tower.bulge")}</span>
        <span role="columnheader">{t("edge.material")}</span>
        <span role="columnheader">{t("tower.pitch")}</span>
        <span role="columnheader"></span>
      </div>
      <!-- Top tier first, as the tower is seen. -->
      {#each tower.tiers.map((x, i) => ({ x, i })).reverse() as { x, i } (i)}
        {@const r = preview?.result.tiers[i]}
        <div class="tr" role="row" data-testid="tower-tier">
          <span class="no" role="cell">{i + 1}</span>
          <span role="cell"><span class="unit"><input type="text" aria-label="{t('tower.tierN', { n: i + 1 })}: {t('tower.rise')}" use:numeric={{ value: x.rise, step: 0.1, min: 0, max: 100, live: true, onchange: (v) => setTier(i, { rise: v }) }} /><em>m</em></span></span>
          <span role="cell"><span class="unit"><input type="text" aria-label="{t('tower.tierN', { n: i + 1 })}: {t('tower.topWidth')}" use:numeric={{ value: x.top_width, step: 0.1, min: 0, max: 200, live: true, onchange: (v) => setTier(i, { top_width: v }) }} /><em>m</em></span></span>
          <span role="cell"><span class="unit"><input type="text" aria-label="{t('tower.tierN', { n: i + 1 })}: {t('tower.bulge')}" use:numeric={{ value: x.bulge ?? 0, step: 0.05, min: -100, max: 100, live: true, onchange: (v) => setTier(i, { bulge: v }) }} /><em>m</em></span></span>
          <span role="cell">
            <select aria-label="{t('tower.tierN', { n: i + 1 })}: {t('edge.material')}" value={x.material ?? 2} onchange={(e) => setTier(i, { material: +(e.target as HTMLSelectElement).value })}>
              {#each materialIds(project) as mt}<option value={mt}>{materialName(project, mt)}</option>{/each}
            </select>
          </span>
          <span class="ro" role="cell">{r ? (r.kind === "wall" ? "90°" : deg(r.pitch_deg)) : "—"}</span>
          <span class="acts" role="cell">
            <button class="icon-btn" title={t("tower.tierUp", { n: i + 1 })} aria-label={t("tower.tierUp", { n: i + 1 })} disabled={i >= tower.tiers.length - 1} onclick={() => moveTier(i, 1)}><Icon name="up" size={15} /></button>
            <button class="icon-btn" title={t("tower.tierDown", { n: i + 1 })} aria-label={t("tower.tierDown", { n: i + 1 })} disabled={i === 0} onclick={() => moveTier(i, -1)}><Icon name="down" size={15} /></button>
            <button class="icon-btn" title={t("tower.removeTier", { n: i + 1 })} aria-label={t("tower.removeTier", { n: i + 1 })} disabled={tower.tiers.length <= 1} onclick={() => removeTier(i)}><Icon name="trash" size={15} /></button>
          </span>
        </div>
      {/each}
    </div>
  {/key}

  {#if blocked}
    <p class="note" role="alert" data-testid="tower-blocked"><i></i><span>{blocked}</span></p>
  {/if}
</div>

<style>
  .dlg {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 6px;
  }
  .preset {
    padding: 7px 4px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
    line-height: 1.2;
    cursor: pointer;
  }
  .preset:hover {
    border-color: var(--accent);
  }
  .preset:focus-visible,
  .icon-btn:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .body {
    display: grid;
    grid-template-columns: 1fr 220px;
    gap: 16px;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 10px 12px;
    align-content: start;
  }
  .fields .wide {
    grid-column: span 2;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .fields input,
  .fields select {
    width: 100%;
  }
  select {
    height: 34px;
  }
  .unit {
    position: relative;
    display: block;
  }
  .unit input {
    width: 100%;
    padding-right: 24px;
    font-variant-numeric: tabular-nums;
  }
  .unit em {
    position: absolute;
    right: 8px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
  }
  .view {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  figcaption {
    font-size: 11.5px;
    color: var(--text-2);
  }
  svg,
  .empty {
    width: 100%;
    height: 190px;
    background: var(--plan-fill);
    border-radius: 6px;
  }
  .roof {
    fill: color-mix(in srgb, var(--accent) 12%, transparent);
    stroke: var(--accent);
    stroke-width: 1.6;
    stroke-linejoin: round;
  }
  .shaft {
    stroke: var(--text-2);
    stroke-width: 1.4;
  }
  .levels {
    stroke: var(--text-3);
    stroke-width: 1;
  }
  .axis {
    stroke: var(--text-3);
    stroke-dasharray: 3 3;
  }
  .facts {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12.5px;
  }
  .facts div {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .facts dt {
    color: var(--text-2);
  }
  .facts dd {
    margin: 0;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .tiers-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
  }
  .btn.sm {
    height: 30px;
    padding: 0 10px;
    font-size: 12.5px;
  }
  .tiers {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 260px;
    overflow-y: auto;
  }
  .tr {
    display: grid;
    grid-template-columns: 24px repeat(3, minmax(78px, 1fr)) minmax(100px, 1.3fr) 48px 96px;
    gap: 6px;
    align-items: center;
  }
  .tr select {
    width: 100%;
  }
  .th {
    font-size: 11.5px;
    color: var(--text-2);
  }
  .no,
  .ro {
    font-variant-numeric: tabular-nums;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .acts {
    display: flex;
    gap: 2px;
  }
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
  }
  .icon-btn:hover:not(:disabled) {
    border-color: var(--border);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .note {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .note i {
    width: 8px;
    height: 8px;
    flex: none;
    background: var(--danger);
  }
  @media (max-width: 760px) {
    .presets {
      grid-template-columns: repeat(3, 1fr);
    }
    .body {
      grid-template-columns: 1fr;
    }
  }
</style>
