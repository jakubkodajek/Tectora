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
  import { hscroll } from "./hscroll";
  import { partLabel } from "./partname";
  import { prefs, presets, savePrefs } from "./settings.svelte";

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
  const partName = $derived(part && selection ? partLabel(part.name, selection.part) : "");
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

  /**
   * The first row holds what a roofer sets on every wall (eave or gable,
   * pitch, eave height, overhang); the rest is in the "More" row, opened
   * per user (UX-008).
   */
  const moreOpen = $derived(!!prefs.wallMore);
  function toggleMore() {
    prefs.wallMore = !prefs.wallMore;
    savePrefs();
  }
  /** What the selected wall has set in the "More" row, so a closed row hides nothing silently. */
  const moreSet = $derived.by(() => {
    if (!edge || marker) return [] as string[];
    const out: string[] = [];
    if (onheight && buildingHeight === null) out.push(t("edge.eaveHeight"));
    if (gable && edge.clad) out.push(t("edge.clad"));
    if (gable && edge.half_hip) out.push(t("edge.halfHip"));
    if (!gable && edge.priority) out.push(t("edge.priority"));
    if ((!gable || edge.half_hip) && breaks.length) out.push(`${t("edge.breaks")} (${breaks.length})`);
    if ((!gable || edge.clad) && (edge.material ?? 1) !== 1) out.push(`${t("edge.material")}: ${materialName(project, edge.material ?? 1)}`);
    return out;
  });
  const moreTitle = $derived(moreSet.length ? `${t("bar.moreHint")}\n${t("bar.moreSet", { list: moreSet.join(", ") })}` : t("bar.moreHint"));

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
  <div class="row" use:hscroll>
    <div class="nav">
      <button class="icon-btn" onclick={() => step(-1)} title={t("def.prev")} aria-label={t("def.prev")}><Icon name="left" /></button>
      <div class="which">
        {#if selection && part}
          <strong>{t("def.wall", { n: selection.edge + 1 })}</strong>
          <span>{t("def.of", { n: part.edges.length })}{project.parts.length > 1 ? ` · ${partName}` : ""}</span>
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
          <div class="unit"><input type="text" use:numeric={{ value: directionDeg, step: 1, onchange: setDirection }} /><em>°</em></div>
        </label>
        <button class="icon-btn" onclick={() => setDirection(directionDeg + 180)} title={t("edge.flip")} aria-label={t("edge.flip")}><Icon name="refresh" /></button>
      {:else}
        <div class="seg" role="radiogroup" aria-label={t("edge.type")}>
          <button class:on={!gable} role="radio" aria-checked={!gable} onclick={() => setType(false)}>{t("edge.eave")}</button>
          <button class:on={gable} role="radio" aria-checked={gable} onclick={() => setType(true)}>{t("edge.gable")}</button>
        </div>
        <label class:off={gable} title={edge.pitch_deg > GABLE_PITCH && !gable ? t("edge.leaningHint") : undefined}>
          <span>{t("edge.pitch")}</span>
          <div class="unit"><input type="text" disabled={gable} use:numeric={{ value: gable ? null : edge.pitch_deg, step: 1, min: 2, max: 178, onchange: setPitch }} /><em>°</em></div>
        </label>
      {/if}
      {#if onheight}
        <label class="wrap" title={buildingHeight === null ? `${t("edge.buildingHeightMixed", { name: partName })} ${t("edge.buildingHeightHint", { name: partName })}` : t("edge.buildingHeightHint", { name: partName })}>
          <span>{t("edge.buildingHeight")}</span>
          <div class="unit"><input type="text" data-testid="building-height" placeholder={buildingHeight === null ? t("edge.mixed") : undefined}
            use:numeric={{ value: buildingHeight, step: 0.05, min: 0, max: 1000, onchange: onheight }} /><em>m</em></div>
        </label>
      {:else}
        <label>
          <span>{t("edge.eaveHeight")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: edge.eave_height, step: 0.05, min: 0, onchange: (v) => onchange({ eave_height: v }) }} /><em>m</em></div>
        </label>
      {/if}
      <label>
        <span>{t("edge.overhang")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: edge.overhang, step: 0.05, min: 0, onchange: (v) => onchange({ overhang: v }) }} /><em>m</em></div>
      </label>
      <button class="btn sm more-btn" class:on={moreOpen} data-testid="def-more" aria-expanded={moreOpen} aria-controls="def-more-row"
        aria-describedby={moreSet.length ? "def-more-set" : undefined}
        title={moreTitle} onclick={toggleMore}>
        {t("bar.more")}
        {#if moreSet.length}<b class="count" data-testid="def-more-count">{moreSet.length}</b><span class="sr-only" id="def-more-set">{t("bar.moreSet", { list: moreSet.join(", ") })}</span>{/if}
        <Icon name={moreOpen ? "x" : "right"} size={12} />
      </button>
    {:else}
      <p class="tip">{t("editor.tip")}</p>
    {/if}
  </div>

  {#if edge && part && moreOpen}
    <div class="row more" id="def-more-row" use:hscroll>
      {#if onheight}
        <label>
          <span>{t("edge.eaveHeight")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: edge.eave_height, step: 0.05, min: 0, onchange: (v) => onchange({ eave_height: v }) }} /><em>m</em></div>
        </label>
      {/if}
      {#if !marker}
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
      {#if !marker}
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
              <div class="unit"><input type="text" use:numeric={{ value: edge.half_hip.pitch_deg, step: 1, min: 2, max: 89.9, onchange: (v) => { if (edge.half_hip) onchange({ half_hip: { ...edge.half_hip, pitch_deg: v } }); } }} /><em>°</em></div>
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
                <div class="unit"><input type="text" use:numeric={{ value: b.height, step: 0.05, min: edge.eave_height, onchange: (v) => setBreak(i, { height: v }) }} /><em>m</em></div>
              </label>
              <label>
                <span>{t("edge.breakPitch")}</span>
                <div class="unit"><input type="text" use:numeric={{ value: b.pitch_deg, step: 1, min: 2, max: 90, onchange: (v) => setBreak(i, { pitch_deg: v }) }} /><em>°</em></div>
              </label>
              <button class="icon-btn" onclick={() => onchange({ breaks: breaks.filter((_, j) => j !== i).map((x) => ({ ...x })) })}
                title={t("edge.removeBreak")} aria-label={t("edge.removeBreak")}><Icon name="x" /></button>
            </div>
          {/each}
          <button class="btn sm" onclick={addBreak} title={t("edge.addBreakHint")}><Icon name="plus" /> {t("edge.addBreak")}</button>
        {/if}
      {/if}
      <div class="actions">
        {#if !gable && !marker}
          <button class="btn sm" onclick={() => onapplyall({ pitch_deg: edge.pitch_deg })} title={t("edge.pitchAllHint")}>{t("edge.pitchAll")}</button>
        {/if}
        <button class="btn sm" onclick={() => onapplyall({ eave_height: edge.eave_height, overhang: edge.overhang })}>{t("edge.heightAll")}</button>
        {#if !gable || edge.clad}
          <button class="btn sm" onclick={() => onapplyall({ material: edge.material ?? 1 })} title={t("edge.materialAllHint")}>{t("edge.materialAll")}</button>
        {/if}
      </div>
      <div class="actions">
        {#if oncalc && !marker && !gable}
          <button class="btn sm" onclick={oncalc} title={t("calc.buttonHint")}>{t("calc.button")}</button>
        {/if}
        {#if onround && !marker}
          <button class="btn sm" onclick={onround} title={t("round.hint")}>{t("round.button")}</button>
        {/if}
        {#if !marker}
          <button class="btn sm" onclick={onmarker} title={t("edge.addMarkerHint")}>{t("edge.addMarker")}</button>
        {/if}
      </div>
      <div class="actions">
        <button class="icon-btn danger" disabled={!marker && realWalls <= 3} onclick={ondelete} title={t("edge.deleteVertex")} aria-label={t("edge.deleteVertex")}><Icon name="trash" /></button>
      </div>
    </div>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px 10px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    min-height: 70px;
    min-width: 0;
  }
  /* One line each: a narrow window scrolls the row sideways instead of
     wrapping it, so the bar stays at most two rows high (UI-008). */
  .row {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    overflow-x: auto;
    padding-bottom: 2px;
  }
  .row > :global(*) {
    flex-shrink: 0;
  }
  .row.more {
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  /* Stays in view at the row's end when a narrow window scrolls the row. */
  .more-btn {
    position: sticky;
    right: 0;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 1px;
    margin-left: auto;
    background: var(--surface);
    box-shadow: -8px 0 8px -4px var(--surface);
  }
  .more-btn.on {
    background: var(--accent-soft);
    border-color: var(--accent-soft);
  }
  .count {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11.5px;
    font-weight: 700;
    line-height: 18px;
    text-align: center;
  }
  .more-btn.on .count {
    background: var(--surface);
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 10px;
    border-right: 1px solid var(--border);
    align-self: stretch;
  }
  .which {
    display: flex;
    flex-direction: column;
    min-width: 64px;
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
  /* A long label wraps within its field, so the first row stays one line. */
  label.wrap span {
    width: 80px;
    white-space: normal;
    line-height: 1.2;
  }
  label.off {
    opacity: 0.4;
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 80px;
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
    align-items: center;
    gap: 8px;
    padding-left: 14px;
    border-left: 1px solid var(--border);
    align-self: stretch;
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
