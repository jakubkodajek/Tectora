<script lang="ts">
  // DACAD's "roof definition" toolbar: the selected wall's roof parameters in
  // one row, with previous/next wall buttons, docked under the views.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import type { EdgeDef, PitchBreak, Project, Selection } from "./types";
  import { GABLE_PITCH, isGable, isMarker } from "./types";
  import { materialIds, materialName } from "./materials";

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
  let lastPitch = 40;

  /** Steps through all walls of all parts, like DACAD's NEXT/PREV buttons. */
  function step(d: number) {
    const all: { part: number; edge: number }[] = project.parts.flatMap((p, pi) => p.edges.map((_, ei) => ({ part: pi, edge: ei })));
    if (all.length === 0) return;
    const cur = selection ? all.findIndex((s) => s.part === selection!.part && s.edge === selection!.edge) : -1;
    selection = all[(cur + d + all.length) % all.length];
  }

  function num(e: Event): number | null {
    const v = parseFloat((e.target as HTMLInputElement).value.replace(",", "."));
    return Number.isFinite(v) ? v : null;
  }

  function setDirection(deg: number) {
    onchange({ direction_rad: (((deg % 360) + 360) % 360) * (Math.PI / 180) });
  }

  /** A pitch for an eave plane: under 90°, or over 90° for a plane leaning out. */
  function setPitch(v: number | null) {
    if (v && v > 0 && v < 180 && Math.abs(v - GABLE_PITCH) > 0.001) onchange({ pitch_deg: v });
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
    if (toGable) {
      lastPitch = edge.pitch_deg;
      onchange({ pitch_deg: GABLE_PITCH });
    } else {
      onchange({ pitch_deg: lastPitch, half_hip: null });
    }
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

  {#if edge && part}
    {#if marker}
      <div class="kind" title={t("edge.markerHint")}>{t("edge.marker")}</div>
      <label>
        <span>{t("edge.direction")}</span>
        <div class="unit"><input type="number" step="1" value={directionDeg}
          onchange={(e) => { const v = num(e); if (v !== null) setDirection(v); }} /><em>°</em></div>
      </label>
      <button class="icon-btn" onclick={() => setDirection(directionDeg + 180)} title={t("edge.flip")} aria-label={t("edge.flip")}><Icon name="refresh" /></button>
    {:else}
      <div class="seg" role="radiogroup" aria-label={t("edge.type")}>
        <button class:on={!gable} role="radio" aria-checked={!gable} onclick={() => setType(false)}>{t("edge.eave")}</button>
        <button class:on={gable} role="radio" aria-checked={gable} onclick={() => setType(true)}>{t("edge.gable")}</button>
      </div>
      <label>
        <span>{t("edge.length")}</span>
        <div class="unit"><input type="number" step="0.05" min="0.05" value={length.toFixed(2)}
          onchange={(e) => { const v = num(e); if (v && v > 0) onlength(v); }} /><em>m</em></div>
      </label>
    {/if}
    {#if onpoint}
      <label class="xy" title={t("edge.startHint")}>
        <span>{t("edge.startX")}</span>
        <div class="unit"><input type="number" step="0.05" value={+edge.start.x.toFixed(3)}
          onchange={(e) => { const v = num(e); if (v !== null) onpoint(v, edge.start.y); }} /><em>m</em></div>
      </label>
      <label class="xy" title={t("edge.startHint")}>
        <span>{t("edge.startY")}</span>
        <div class="unit"><input type="number" step="0.05" value={+edge.start.y.toFixed(3)}
          onchange={(e) => { const v = num(e); if (v !== null) onpoint(edge.start.x, v); }} /><em>m</em></div>
      </label>
    {/if}
    <label class:off={gable} title={edge.pitch_deg > GABLE_PITCH && !gable ? t("edge.leaningHint") : undefined}>
      <span>{t("edge.pitch")}</span>
      <div class="unit"><input type="number" step="1" min="1" max="179" disabled={gable} value={gable ? "" : edge.pitch_deg}
        onchange={(e) => setPitch(num(e))} /><em>°</em></div>
    </label>
    <label>
      <span>{t("edge.eaveHeight")}</span>
      <div class="unit"><input type="number" step="0.05" value={edge.eave_height}
        onchange={(e) => { const v = num(e); if (v !== null) onchange({ eave_height: v }); }} /><em>m</em></div>
    </label>
    <label>
      <span>{t("edge.overhang")}</span>
      <div class="unit"><input type="number" step="0.05" min="0" value={edge.overhang}
        onchange={(e) => { const v = num(e); if (v !== null && v >= 0) onchange({ overhang: v }); }} /><em>m</em></div>
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
          <div class="unit"><input type="number" step="0.05" value={edge.half_hip.height}
            onchange={(e) => { const v = num(e); if (v !== null && edge.half_hip) onchange({ half_hip: { ...edge.half_hip, height: v } }); }} /><em>m</em></div>
        </label>
        <label>
          <span>{t("edge.halfHipPitch")}</span>
          <div class="unit"><input type="number" step="1" min="1" max="89" value={edge.half_hip.pitch_deg}
            onchange={(e) => { const v = num(e); if (v && edge.half_hip) onchange({ half_hip: { ...edge.half_hip, pitch_deg: v } }); }} /><em>°</em></div>
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
            <div class="unit"><input type="number" step="0.05" value={b.height}
              onchange={(e) => { const v = num(e); if (v !== null) setBreak(i, { height: v }); }} /><em>m</em></div>
          </label>
          <label>
            <span>{t("edge.breakPitch")}</span>
            <div class="unit"><input type="number" step="1" min="1" max="90" value={b.pitch_deg}
              onchange={(e) => { const v = num(e); if (v && v > 0 && v <= 90) setBreak(i, { pitch_deg: v }); }} /><em>°</em></div>
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
