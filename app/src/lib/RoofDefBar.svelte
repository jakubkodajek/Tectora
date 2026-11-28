<script lang="ts">
  // DACAD's "roof definition" toolbar: the selected wall's roof parameters in
  // one row, with previous/next wall buttons, docked under the views.
  import Icon from "./Icon.svelte";
  import { freshBar } from "./dblclick";
  import { t } from "./i18n.svelte";
  import { radiogroup } from "./tabs";
  import type { CalcResult, EdgeDef, PitchBreak, Project, Selection } from "./types";
  import { aboveRidge, breakDefault, halfHipDefault, isUnusual, ridgeOf } from "./plausible";
  import { num } from "./format";
  import { GABLE_PITCH, buildingHeight as heightOf, isGable, isMarker } from "./types";
  import { materialIds, materialName } from "./materials";
  import { numeric } from "./numeric.svelte";
  import { OVERHANG, WALL_HEIGHT, within } from "./limits";
  import { hscroll } from "./hscroll";
  import { partLabel } from "./partname";
  import { prefs, presets, savePrefs } from "./settings.svelte";
  import { tick } from "svelte";
  import { stepWall, wallStepKey } from "./wallstep";
  import { eavePitchFor, rememberEavePitch } from "./walltype";

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
    onarc,
    result = null,
    ondraw,
    onstandard,
  }: {
    project: Project;
    /** The roof as calculated, for the ridge height a half-hip or break must stay below (UX-003). */
    result?: CalcResult | null;
    /** An empty job: start drawing a footprint, or open the standard roofs (UX-004). */
    ondraw?: () => void;
    onstandard?: () => void;
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
    /** Curved walls (UX-002): bend this wall, make it a semicircular end, or round the corner at its start. */
    onarc?: (op: "bend" | "apse" | "round") => void;
  } = $props();

  const part = $derived(selection ? project.parts[selection.part] : null);
  const edge = $derived(part && selection ? part.edges[selection.edge] : null);
  /** The wall the fields edit now: a field reused for the next one forgets the one before (UI-403). */
  const wallKey = $derived(selection ? `${selection.part}:${selection.edge}` : "");
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
  /** The building's height (top of its walls); the eave of each wall is its own value and may be lower. */
  const buildingHeight = $derived(part ? heightOf(part) : null);
  /** A flat roof ignores the walls' pitch and eave / gable: those fields are off and say why (UX-009). */
  const flat = $derived(!!project.flat_roof);
  /** A pitch over 90° makes a plane leaning out: said next to the field, not only in a tooltip (UX-010). */
  const leaning = $derived(!!edge && !isGable(edge) && edge.pitch_deg > GABLE_PITCH);
  /** The highest point of this building's calculated roof, m (UX-003). */
  const ridge = $derived(selection ? ridgeOf(result, selection.part) : null);
  const halfHipHigh = $derived(!!edge?.half_hip && aboveRidge(edge.half_hip.height, ridge));
  /** Values allowed but unusually large: a decimal comma may be lost (UX-009). */
  const unusualOverhang = $derived(!!edge && isUnusual("overhang", edge.overhang));
  const unusualEave = $derived(!!edge && isUnusual("height", edge.eave_height));
  const unusualHeight = $derived(isUnusual("height", buildingHeight));
  const unusualLength = $derived(isUnusual("length", length));

  /** Stores a wall value inside `b`, else refuses it with the range (UX-001). */
  function bounded(b: { min: number; max: number }, patch: (v: number) => Partial<EdgeDef>) {
    return (v: number) => {
      const ok = within(v, b);
      if (ok !== true) return ok;
      onchange(patch(v));
      return true;
    };
  }

  /**
   * The first row holds what a roofer sets on every wall (eave or gable,
   * pitch, building height, overhang); the rest, this wall's own eave
   * height first, is in the "More" row, opened
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
    // An eave set lower or higher than the building is said on the button.
    if (onheight && buildingHeight !== null && Math.abs(edge.eave_height - buildingHeight) > 1e-9) out.push(t("edge.eaveHeight"));
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
    const next = stepWall(project, selection, d);
    if (next) selection = next;
  }

  let barEl = $state<HTMLDivElement>();
  // The keys are named as on the keyboard; macOS labels Alt as ⌥.
  const alt = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent) ? "⌥" : "Alt+";

  /**
   * PageDown / PageUp (and Shift+Enter) in a field of the bar: the field
   * stores its value on the wall it was typed for, the bar steps to the
   * next wall and the same field takes the focus again, so "pitch, next
   * wall, pitch" needs no mouse (review UX-006). Keys on the bar's buttons
   * step too; the editor handles them everywhere else.
   */
  $effect(() => {
    const el = barEl;
    if (!el) return;
    el.addEventListener("keydown", onbarkey);
    return () => el.removeEventListener("keydown", onbarkey);
  });

  async function onbarkey(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    const field = el instanceof HTMLInputElement && el.type === "text" ? el : null;
    const d = wallStepKey(e, field !== null || el instanceof HTMLSelectElement);
    if (!d) return;
    e.preventDefault();
    e.stopPropagation();
    const fields = barEl ? [...barEl.querySelectorAll<HTMLInputElement>("input.num-input")] : [];
    const at = field ? fields.indexOf(field) : -1;
    // Blurring commits the typed value before the selection changes.
    if (field) field.blur();
    step(d);
    if (at < 0) return;
    await tick();
    const again = barEl ? [...barEl.querySelectorAll<HTMLInputElement>("input.num-input")][at] : undefined;
    const target = again && !again.disabled ? again : barEl?.querySelector<HTMLInputElement>("input.num-input:not(:disabled)");
    target?.focus();
    target?.select();
  }

  function setDirection(deg: number) {
    onchange({ direction_rad: (((deg % 360) + 360) % 360) * (Math.PI / 180) });
  }

  /**
   * A pitch for an eave plane: under 90°. The field's range ends at 90°, so
   * a pitch over 90° (a plane leaning out) is refused as a typo with the
   * range, as in the core's `edit::check_eave_pitch`; one already in a file
   * keeps its mark (UX-010).
   */
  function setPitch(v: number): boolean | string {
    // 90° is a gable, chosen with the Eave / Gable switch.
    if (Math.abs(v - GABLE_PITCH) <= 0.001) return t("edge.pitch90");
    if (!(v > 0 && v < GABLE_PITCH)) return false;
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
    onchange({ breaks: [...breaks.map((b) => ({ ...b })), { height: breakDefault(top, ridge), pitch_deg: Math.max(5, Math.min(85, last - 20)) }] });
  }

  function setType(toGable: boolean) {
    if (!edge || toGable === gable) return;
    if (!selection || !part) return;
    if (toGable) {
      rememberEavePitch(selection.part, edge);
      onchange({ pitch_deg: GABLE_PITCH });
    } else {
      onchange({ pitch_deg: eaveFallback(), half_hip: null });
    }
  }

  /** The pitch for a gable turned back into an eave (`walltype.ts`, one rule with the context menu and MCP). */
  function eaveFallback(): number {
    if (!selection || !part || !edge) return presets.pitch_deg;
    return eavePitchFor(selection.part, part.edges, selection.edge, presets.pitch_deg);
  }
</script>

<div class="bar" role="toolbar" aria-label={t("def.title")} bind:this={barEl} use:freshBar>
  <!-- More sits beside the scrolled strip, not on it: at 600 px it never lies over a field (UI-1901). -->
  <div class="line">
  <div class="row" use:hscroll>
    <div class="nav">
      <button class="icon-btn" onclick={() => step(-1)} title="{t('def.prev')} (PageUp, {alt}←)" aria-label={t("def.prev")}><Icon name="left" /></button>
      <div class="which">
        {#if selection && part}
          <strong>{t("def.wall", { n: selection.edge + 1 })}</strong>
          <span>{t("def.of", { n: part.edges.length })}{project.parts.length > 1 ? ` · ${partName}` : ""}</span>
        {:else}
          <strong>{t("def.title")}</strong>
          <span>{t("def.none")}</span>
        {/if}
      </div>
      <button class="icon-btn" onclick={() => step(1)} title="{t('def.next')} (PageDown, {alt}→, Shift+Enter)" aria-label={t("def.next")}><Icon name="right" /></button>
    </div>

    {#if edge && part}
      {#if marker}
        <div class="kind" title={t("edge.markerHint")}>{t("edge.marker")}</div>
        <label>
          <span>{t("edge.direction")}</span>
          <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: directionDeg, step: 1, onchange: setDirection }} /><em>°</em></div>
        </label>
        <button class="icon-btn" onclick={() => setDirection(directionDeg + 180)} title={t("edge.flip")} aria-label={t("edge.flip")}><Icon name="refresh" /></button>
      {:else}
        <div class="seg" class:off={flat} role="radiogroup" use:radiogroup aria-label={t("edge.type")} title={flat ? t("edge.flatOff") : undefined}>
          <button class:on={!gable} role="radio" aria-checked={!gable} disabled={flat} onclick={() => setType(false)}>{t("edge.eave")}</button>
          <button class:on={gable} role="radio" aria-checked={gable} disabled={flat} onclick={() => setType(true)}>{t("edge.gable")}</button>
        </div>
        <label class:off={gable || flat} title={flat ? t("edge.flatOff") : leaning ? t("edge.leaningHint") : undefined}>
          <span>{t("edge.pitch")}{#if leaning && !flat}<b class="mark" data-testid="pitch-leaning"><i></i>{t("edge.leaning")}</b>{/if}</span>
          <div class="unit"><input type="text" disabled={gable || flat} aria-describedby={flat ? "def-flat-note" : undefined}
            use:numeric={{ key: wallKey, value: gable ? null : edge.pitch_deg, step: 1, min: 2, max: 90, onchange: setPitch }} /><em>°</em></div>
        </label>
        {#if flat}<p class="flat-note" id="def-flat-note" data-testid="flat-note"><i></i>{t("edge.flatOff")}</p>{/if}
      {/if}
      {#if onheight}
        <label class="wrap" title={unusualHeight ? t("edge.unusualHint", { v: num(buildingHeight ?? 0) }) : t("edge.buildingHeightHint", { name: partName })}>
          <span>{t("edge.buildingHeight")}{#if unusualHeight}<b class="mark" data-testid="unusual-height"><i></i>{t("edge.unusual")}</b>{/if}</span>
          <div class="unit"><input type="text" data-testid="building-height"
            use:numeric={{ key: wallKey, value: buildingHeight, step: 0.05, min: WALL_HEIGHT.min, max: WALL_HEIGHT.max, onchange: onheight }} /><em>m</em></div>
        </label>
      {/if}
      {#if !onheight}
        <label title={unusualEave ? t("edge.unusualHint", { v: num(edge.eave_height) }) : undefined}>
          <span>{t("edge.eaveHeight")}{#if unusualEave}<b class="mark" data-testid="unusual-eave"><i></i>{t("edge.unusual")}</b>{/if}</span>
          <div class="unit"><input type="text" data-testid="eave-height" use:numeric={{ key: wallKey, value: edge.eave_height, step: 0.05, min: 0, onchange: bounded(WALL_HEIGHT, (v) => ({ eave_height: v })) }} /><em>m</em></div>
        </label>
      {/if}
      <label title={unusualOverhang ? t("edge.unusualHint", { v: num(edge.overhang) }) : undefined}>
        <span>{t("edge.overhang")}{#if unusualOverhang}<b class="mark" data-testid="unusual-overhang"><i></i>{t("edge.unusual")}</b>{/if}</span>
        <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: edge.overhang, step: 0.05, min: 0, onchange: bounded(OVERHANG, (v) => ({ overhang: v })) }} /><em>m</em></div>
      </label>
    {:else}
      {#if project.parts.length === 0}
        <!-- An empty job: no wall to pick, so say how to start (UX-004). -->
        <!-- The two ways to start come first, so both stay in the bar at 600 px;
             the tip after them wraps instead of running out of the bar (UI-303). -->
        {#if ondraw}<button class="btn sm" onclick={ondraw}><Icon name="draw" size={15} />{t("tool.draw")}</button>{/if}
        {#if onstandard}<button class="btn sm" onclick={onstandard}><Icon name="house" size={15} />{t("tool.standard")}</button>{/if}
        <p class="tip empty" data-testid="empty-tip">{t("editor.empty")}</p>
      {:else}
        <p class="tip" data-testid="def-tip">{t("editor.tip")}</p>
      {/if}
    {/if}
  </div>
    {#if edge && part}
      <button class="btn sm more-btn" class:on={moreOpen} data-testid="def-more" aria-expanded={moreOpen} aria-controls="def-more-row"
      aria-describedby={moreSet.length ? "def-more-set" : undefined}
      title={moreTitle} onclick={toggleMore}>
      {t("bar.more")}
      {#if moreSet.length}<b class="count" data-testid="def-more-count">{moreSet.length}</b><span class="sr-only" id="def-more-set">{t("bar.moreSet", { list: moreSet.join(", ") })}</span>{/if}
    </button>
    {/if}
  </div>

  {#if edge && part && moreOpen}
    <div class="row more" id="def-more-row" use:hscroll>
      {#if onheight}
        <!-- This wall's eave: it may lie below the building height (an eave
             with overhang) or above it; the main row keeps one row at
             960x540 (review UI-003). -->
        <label title={unusualEave ? t("edge.unusualHint", { v: num(edge.eave_height) }) : undefined}>
          <span title={t("edge.eaveHeightHint")}>{t("edge.eaveHeight")}{#if unusualEave}<b class="mark" data-testid="unusual-eave"><i></i>{t("edge.unusual")}</b>{/if}</span>
          <div class="unit"><input type="text" data-testid="eave-height" aria-describedby="def-eave-note" use:numeric={{ key: wallKey, value: edge.eave_height, step: 0.05, min: 0, onchange: bounded(WALL_HEIGHT, (v) => ({ eave_height: v })) }} /><em>m</em></div>
        </label>
        <!-- Said on screen, not only in the tooltip: this is the eave the roof
             is calculated from, the value the defaults set (UX-008). -->
        <p class="eave-note" id="def-eave-note" data-testid="eave-note">{t("edge.eaveHeightNote")}</p>
      {/if}
      {#if !marker}
        <label title={unusualLength ? t("edge.unusualHint", { v: num(length) }) : t("edge.lengthHint")}>
          <span>{t("edge.length")}{#if unusualLength}<b class="mark" data-testid="unusual-length"><i></i>{t("edge.unusual")}</b>{/if}</span>
          <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: length, digits: 2, step: 0.05, min: 0.01, onchange: onlength }} /><em>m</em></div>
        </label>
      {/if}
      {#if onpoint}
        <label class="xy" title={t("edge.startHint")}>
          <span>{t("edge.startX")}</span>
          <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: edge.start.x, step: 0.05, onchange: (v) => onpoint(v, edge.start.y) }} /><em>m</em></div>
        </label>
        <label class="xy" title={t("edge.startHint")}>
          <span>{t("edge.startY")}</span>
          <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: edge.start.y, step: 0.05, onchange: (v) => onpoint(edge.start.x, v) }} /><em>m</em></div>
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
              onchange={(e) => onchange({ half_hip: (e.target as HTMLInputElement).checked ? { height: halfHipDefault(edge, ridge), pitch_deg: 50 } : null })} />
            {t("edge.halfHip")}
          </label>
          {#if edge.half_hip}
            <label title={halfHipHigh ? t("edge.halfHipAboveRidgeHint", { v: num(ridge ?? 0) }) : undefined}>
              <span>{t("edge.halfHipHeight")}{#if halfHipHigh}<b class="mark" data-testid="halfhip-above-ridge"><i></i>{t("edge.aboveRidge")}</b>{/if}</span>
              <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: edge.half_hip.height, step: 0.05, min: WALL_HEIGHT.min, max: WALL_HEIGHT.max, onchange: (v) => { if (edge.half_hip) onchange({ half_hip: { ...edge.half_hip, height: v } }); } }} /><em>m</em></div>
            </label>
            <label>
              <span>{t("edge.halfHipPitch")}</span>
              <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: edge.half_hip.pitch_deg, step: 1, min: 2, max: 89.9, onchange: (v) => { if (edge.half_hip) onchange({ half_hip: { ...edge.half_hip, pitch_deg: v } }); } }} /><em>°</em></div>
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
              <label title={aboveRidge(b.height, ridge) ? t("edge.breakAboveRidgeHint", { v: num(ridge ?? 0) }) : undefined}>
                <span>{t("edge.breakHeight", { n: i + 1 })}{#if aboveRidge(b.height, ridge)}<b class="mark" data-testid="break-above-ridge"><i></i>{t("edge.aboveRidge")}</b>{/if}</span>
                <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: b.height, step: 0.05, min: edge.eave_height, max: WALL_HEIGHT.max, onchange: (v) => setBreak(i, { height: v }) }} /><em>m</em></div>
              </label>
              <label>
                <span>{t("edge.breakPitch")}</span>
                <div class="unit"><input type="text" use:numeric={{ key: wallKey, value: b.pitch_deg, step: 1, min: 2, max: 90, onchange: (v) => setBreak(i, { pitch_deg: v }) }} /><em>°</em></div>
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
          <button class="btn sm" disabled={flat} onclick={() => onapplyall({ pitch_deg: edge.pitch_deg })} title={flat ? t("edge.flatOff") : t("edge.pitchAllHint")}>{t("edge.pitchAll")}</button>
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
      {#if onarc && !marker}
        <div class="actions" data-testid="def-arcs">
          <button class="btn sm" onclick={() => onarc("bend")}>{t("arc.bend")}</button>
          <button class="btn sm" onclick={() => onarc("apse")}>{t("arc.apse")}</button>
          <button class="btn sm" onclick={() => onarc("round")} title={t("arc.roundBarHint")}>{t("arc.roundStart")}</button>
        </div>
      {/if}
      <div class="actions">
        <button class="btn sm ghost danger" disabled={!marker && realWalls <= 3} onclick={ondelete}
          title={marker ? t("edge.deleteMarker") : t("edge.deleteVertexHint")}><Icon name="trash" size={15} />{marker ? t("edge.deleteMarker") : t("edge.deleteVertex")}</button>
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
  .row.more .unit input {
    width: 92px;
  }
  /* The first row and More: the row scrolls, More stays beside it at the
     end, never over a field (UI-1901). */
  .line {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    min-width: 0;
  }
  .line > .row {
    flex: 1 1 auto;
    min-width: 0;
  }
  .more-btn {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 3px;
    margin-left: auto;
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
  .eave-note {
    margin: 0 0 7px;
    max-width: 180px;
    font-size: 11.5px;
    line-height: 1.25;
    color: var(--text-2);
    white-space: normal;
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
    width: 84px;
    white-space: normal;
    line-height: 1.2;
  }
  label.off,
  .seg.off {
    opacity: 0.4;
  }
  /* Status marks: a small dot and a word, no tinted panel (AGENTS.md). */
  .mark {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 6px;
    font-weight: 600;
    color: var(--text);
  }
  .mark i,
  .flat-note i {
    width: 6px;
    height: 6px;
    border-radius: 1px;
    background: var(--ai);
    flex: none;
  }
  .flat-note i {
    background: var(--text-3);
  }
  .flat-note {
    display: flex;
    align-items: center;
    gap: 6px;
    align-self: center;
    max-width: 220px;
    margin: 0;
    font-size: 12px;
    line-height: 1.25;
    color: var(--text-2);
  }
  .unit {
    position: relative;
    /* As wide as the field, so the unit stays at its right edge under a wider label (UX-010 mark). */
    width: max-content;
  }
  .unit input {
    width: 84px;
    padding-right: 24px;
    padding-left: 8px;
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
  /* A tip (the empty job's, and the one with no wall selected) takes the
     room that is left and wraps in it, never cut mid-word in the strip
     (UI-303, audit 2026-11-24 UI-2001). */
  .row > .tip {
    flex: 1 1 0;
    min-width: 120px;
    line-height: 1.25;
  }
</style>
