<script lang="ts" module>
  /**
   * The pitch each wall had before it became a gable, so Eave brings back
   * that wall's own pitch (not the last one switched). Keyed by part and
   * the wall's start corner, which survive undo and reloads; kept for the
   * session, also while another bar is shown.
   */
  const eavePitch = new Map<string, number>();
  const wallKey = (part: number, e: { start: { x: number; y: number } }) => `${part}:${e.start.x},${e.start.y}`;
</script>

<script lang="ts">
  // DACAD's "roof definition" toolbar: the selected wall's roof parameters in
  // one row, with previous/next wall buttons, docked under the views.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import type { EdgeDef, PitchBreak, Project, Selection } from "./types";
  import { GABLE_PITCH, isGable, isMarker } from "./types";
  import { materialIds, materialName } from "./materials";
  import { numeric } from "./numeric.svelte";
  import { presets } from "./settings.svelte";

  let {
    project,
    selection = $bindable(),
    onchange,
    onapplyall,
    onlength,
    onpoint,
    ondelete,
    onmarker,
    onround,
    oncalc,
    onheight,
  }: {
    project: Project;
    selection: Selection;
    onchange: (patch: Partial<EdgeDef>) => void;
    onapplyall: (patch: Partial<EdgeDef>) => void;
    onlength: (len: number) => void;
    /** Moves the wall's start point to typed coordinates (DACAD: Punkt verschieben). */
    onpoint?: (x: number, y: number) => void;
    ondelete: () => void;
    onmarker: () => void;
    /** Opens the round-roof helper; absent when the wall has no opposite wall. */
    onround?: () => void;
    /** Opens the pitch / eave / overhang calculations for the wall. */
    oncalc?: () => void;
    /** Sets the eave height of every wall of the selected building at once. */
    onheight?: (h: number) => unknown;
  } = $props();

  const part = $derived(selection ? project.parts[selection.part] : null);
  const edge = $derived(part && selection ? part.edges[selection.edge] : null);
  const length = $derived.by(() => {
    if (!part || !selection || !edge) return 0;
    const next = part.edges[(selection.edge + 1) % part.edges.length];
    return Math.hypot(next.start.x - edge.start.x, next.start.y - edge.start.y);
  });
  const gable = $derived(!!edge && isGable(edge));
  const marker = $derived(!!edge && isMarker(edge));
  const realWalls = $derived(part ? part.edges.filter((e) => !isMarker(e)).length : 0);
  const directionDeg = $derived(edge?.direction_rad != null ? +(((edge.direction_rad * 180) / Math.PI + 360) % 360).toFixed(2) : 0);
  const breaks = $derived(edge?.breaks ?? []);
  /** The eave height all walls of the building share; null when they differ. */
  const buildingHeight = $derived.by(() => {
    if (!part || part.edges.length === 0) return null;
    const h = part.edges[0].eave_height;
    return part.edges.every((e) => e.eave_height === h) ? h : null;
  });

  /** Steps through all walls of all parts, like DACAD's NEXT/PREV buttons. */
  function step(d: number) {
    const all: { part: number; edge: number }[] = project.parts.flatMap((p, pi) => p.edges.map((_, ei) => ({ part: pi, edge: ei })));
    if (all.length === 0) return;
    const cur = selection ? all.findIndex((s) => s.part === selection!.part && s.edge === selection!.edge) : -1;
    selection = all[(cur + d + all.length) % all.length];
  }

  function setDirection(deg: number) {
    onchange({ direction_rad: (((deg % 360) + 360) % 360) * (Math.PI / 180) });
  }

  /** A pitch for an eave plane: under 90°, or over 90° for a plane leaning out. */
  function setPitch(v: number): boolean | string {
    // 90° is a gable, chosen with the Eave / Gable switch.
    if (Math.abs(v - GABLE_PITCH) <= 0.001) return t("edge.pitch90");
    if (!(v > 0 && v < 180)) return false;
    onchange({ pitch_deg: v });
    return true;
  }

  function setBreak(i: number, patch: Partial<PitchBreak>) {
    onchange({ breaks: breaks.map((b, j) => (j === i ? { ...b, ...patch } : { ...b })) });
  }

  function addBreak() {
    if (!edge) return;
    const top = breaks.reduce((h, b) => Math.max(h, b.height), edge.eave_height);
    const last = breaks.length ? breaks[breaks.length - 1].pitch_deg : edge.pitch_deg;
    onchange({ breaks: [...breaks.map((b) => ({ ...b })), { height: +(top + 1.5).toFixed(2), pitch_deg: Math.max(5, Math.min(85, last - 20)) }] });
  }

  function setType(toGable: boolean) {
    if (!edge || toGable === gable) return;
    if (!selection || !part) return;
    if (toGable) {
      eavePitch.set(wallKey(selection.part, edge), edge.pitch_deg);
      onchange({ pitch_deg: GABLE_PITCH });
    } else {
      onchange({ pitch_deg: eaveFallback(), half_hip: null });
    }
  }

  /** The pitch for a gable turned back into an eave: its own from before,
   *  else the nearest eave wall's in this part, else the roof default. */
  function eaveFallback(): number {
    if (!selection || !part || !edge) return presets.pitch_deg;
    const own = eavePitch.get(wallKey(selection.part, edge));
    if (own !== undefined) return own;
    const n = part.edges.length;
    for (let d = 1; d <= n / 2; d++) {
      for (const k of [selection.edge - d, selection.edge + d]) {
        const e = part.edges[((k % n) + n) % n];
        if (!isGable(e) && !isMarker(e)) return e.pitch_deg;
      }
    }
    return presets.pitch_deg;
  }
</script>

<div class="bar" role="toolbar" aria-label={t("def.title")}>
  <div class="nav">
    <button class="icon-btn" onclick={() => step(-1)} title={t("def.prev")} aria-label={t("def.prev")}><Icon name="left" /></button>
    <div class="which">
      {#if selection && part}
        <strong>{t("def.wall", { n: selection.edge + 1 })}</strong>
        <span>{t("def.of", { n: part.edges.length })}{project.parts.length > 1 ? ` · ${part.name}` : ""}</span>
      {:else}
        <strong>{t("def.title")}</strong>
        <span>{t("def.none")}</span>
      {/if}
    </div>
    <button class="icon-btn" onclick={() => step(1)} title={t("def.next")} aria-label={t("def.next")}><Icon name="right" /></button>
  </div>

  {#if part && onheight}
    <div class="building">
      <label title={buildingHeight === null ? `${t("edge.buildingHeightMixed", { name: part.name })} ${t("edge.buildingHeightHint", { name: part.name })}` : t("edge.buildingHeightHint", { name: part.name })}>
        <span>{t("edge.buildingHeight")}</span>
        <div class="unit"><input type="text" data-testid="building-height" placeholder={buildingHeight === null ? t("edge.mixed") : undefined}
          use:numeric={{ value: buildingHeight, step: 0.05, min: 0, max: 1000, onchange: onheight }} /><em>m</em></div>
      </label>
    </div>
  {/if}

  {#if edge && part}
    {#if marker}
      <div class="kind" title={t("edge.markerHint")}>{t("edge.marker")}</div>
      <label>
        <span>{t("edge.direction")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: directionDeg, step: 1, onchange: setDirection }} /><em>°</em></div>
      </label>
      <button class="icon-btn" onclick={() => setDirection(directionDeg + 180)} title={t("edge.flip")} aria-label={t("edge.flip")}><Icon name="refresh" /></button>
    {:else}
      <div class="seg" role="radiogroup" aria-label={t("edge.type")}>
        <button class:on={!gable} role="radio" aria-checked={!gable} onclick={() => setType(false)}>{t("edge.eave")}</button>
        <button class:on={gable} role="radio" aria-checked={gable} onclick={() => setType(true)}>{t("edge.gable")}</button>
      </div>
      <label>
        <span>{t("edge.length")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: length, digits: 2, step: 0.05, min: 0.01, onchange: onlength }} /><em>m</em></div>
      </label>
    {/if}
    {#if onpoint}
      <label class="xy" title={t("edge.startHint")}>
        <span>{t("edge.startX")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: edge.start.x, step: 0.05, onchange: (v) => onpoint(v, edge.start.y) }} /><em>m</em></div>
      </label>
      <label class="xy" title={t("edge.startHint")}>
        <span>{t("edge.startY")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: edge.start.y, step: 0.05, onchange: (v) => onpoint(edge.start.x, v) }} /><em>m</em></div>
      </label>
    {/if}
    <label class:off={gable} title={edge.pitch_deg > GABLE_PITCH && !gable ? t("edge.leaningHint") : undefined}>
      <span>{t("edge.pitch")}</span>
      <div class="unit"><input type="text" disabled={gable} use:numeric={{ value: gable ? null : edge.pitch_deg, step: 1, min: 0.1, max: 179.9, onchange: setPitch }} /><em>°</em></div>
    </label>
    <label>
      <span>{t("edge.eaveHeight")}</span>
      <div class="unit"><input type="text" use:numeric={{ value: edge.eave_height, step: 0.05, onchange: (v) => onchange({ eave_height: v }) }} /><em>m</em></div>
    </label>
    <label>
      <span>{t("edge.overhang")}</span>
      <div class="unit"><input type="text" use:numeric={{ value: edge.overhang, step: 0.05, min: 0, onchange: (v) => onchange({ overhang: v }) }} /><em>m</em></div>
    </label>
    {#if !gable || edge.clad}
      <label title={t("edge.materialHint")}>
        <span>{t("edge.material")}</span>
        <select value={edge.material ?? 1}
          onchange={(e) => onchange({ material: +(e.target as HTMLSelectElement).value })}>
          {#each materialIds(project) as m}
            <option value={m}>{materialName(project, m)}</option>
          {/each}
        </select>
      </label>
    {/if}
    {#if gable}
      <label class="check" title={t("edge.cladHint")}>
        <input type="checkbox" checked={!!edge.clad}
          onchange={(e) => onchange({ clad: (e.target as HTMLInputElement).checked })} />
        {t("edge.clad")}
      </label>
      <label class="check">
        <input type="checkbox" checked={!!edge.half_hip}
          onchange={(e) => onchange({ half_hip: (e.target as HTMLInputElement).checked ? { height: edge.eave_height + 2.5, pitch_deg: 50 } : null })} />
        {t("edge.halfHip")}
      </label>
      {#if edge.half_hip}
        <label>
          <span>{t("edge.halfHipHeight")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: edge.half_hip.height, step: 0.05, onchange: (v) => { if (edge.half_hip) onchange({ half_hip: { ...edge.half_hip, height: v } }); } }} /><em>m</em></div>
        </label>
        <label>
          <span>{t("edge.halfHipPitch")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: edge.half_hip.pitch_deg, step: 1, min: 0.1, max: 89.9, onchange: (v) => { if (edge.half_hip) onchange({ half_hip: { ...edge.half_hip, pitch_deg: v } }); } }} /><em>°</em></div>
        </label>
      {/if}
    {/if}
    {#if !gable}
      <label class="check" title={t("edge.priorityHint")}>
        <input type="checkbox" checked={!!edge.priority}
          onchange={(e) => onchange({ priority: (e.target as HTMLInputElement).checked ? 1 : 0 })} />
        {t("edge.priority")}
      </label>
    {/if}
    {#if !gable || edge.half_hip}
      {#each breaks as b, i}
        <div class="break">
          <label>
            <span>{t("edge.breakHeight", { n: i + 1 })}</span>
            <div class="unit"><input type="text" use:numeric={{ value: b.height, step: 0.05, onchange: (v) => setBreak(i, { height: v }) }} /><em>m</em></div>
          </label>
          <label>
            <span>{t("edge.breakPitch")}</span>
            <div class="unit"><input type="text" use:numeric={{ value: b.pitch_deg, step: 1, min: 0.1, max: 90, onchange: (v) => setBreak(i, { pitch_deg: v }) }} /><em>°</em></div>
          </label>
          <button class="icon-btn" onclick={() => onchange({ breaks: breaks.filter((_, j) => j !== i).map((x) => ({ ...x })) })}
            title={t("edge.removeBreak")} aria-label={t("edge.removeBreak")}><Icon name="x" /></button>
        </div>
      {/each}
      <button class="btn sm" onclick={addBreak} title={t("edge.addBreakHint")}><Icon name="plus" /> {t("edge.addBreak")}</button>
    {/if}
    <div class="spacer"></div>
    <div class="actions">
      {#if !gable && !marker}
        <button class="btn sm" onclick={() => onapplyall({ pitch_deg: edge.pitch_deg })} title={t("edge.pitchAllHint")}>{t("edge.pitchAll")}</button>
      {/if}
      <button class="btn sm" onclick={() => onapplyall({ eave_height: edge.eave_height, overhang: edge.overhang })}>{t("edge.heightAll")}</button>
      {#if !gable || edge.clad}
        <button class="btn sm" onclick={() => onapplyall({ material: edge.material ?? 1 })} title={t("edge.materialAllHint")}>{t("edge.materialAll")}</button>
      {/if}
      {#if oncalc && !marker && !gable}
        <button class="btn sm" onclick={oncalc} title={t("calc.buttonHint")}>{t("calc.button")}</button>
      {/if}
      {#if onround && !marker}
        <button class="btn sm" onclick={onround} title={t("round.hint")}>{t("round.button")}</button>
      {/if}
      {#if !marker}
        <button class="btn sm" onclick={onmarker} title={t("edge.addMarkerHint")}>{t("edge.addMarker")}</button>
      {/if}
      <button class="icon-btn danger" disabled={!marker && realWalls <= 3} onclick={ondelete} title={t("edge.deleteVertex")} aria-label={t("edge.deleteVertex")}><Icon name="trash" /></button>
    </div>
  {:else}
    <p class="tip">{t("editor.tip")}</p>
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
    overflow-x: auto;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 14px;
    border-right: 1px solid var(--border);
    align-self: stretch;
  }
  .building {
    display: flex;
    align-items: flex-end;
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
  .seg {
    display: flex;
    padding: 3px;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .seg button {
    height: 28px;
    padding: 0 12px;
    border: none;
    border-radius: 7px;
    background: transparent;
    font-weight: 500;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
  }
  label.off {
    opacity: 0.4;
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
  .check {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 6px;
    height: 34px;
    white-space: nowrap;
  }
  .check input {
    width: auto;
  }
  .spacer {
    flex: 1;
  }
  .kind {
    align-self: center;
    font-weight: 600;
    white-space: nowrap;
  }
  .break {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    padding-left: 10px;
    border-left: 1px solid var(--border);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .btn.sm {
    height: 32px;
    padding: 0 10px;
    font-size: 13px;
    white-space: nowrap;
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
