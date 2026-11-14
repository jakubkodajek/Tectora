<script lang="ts">
  // DACAD's dormer dialog (Gaubeneingabe, Ctrl+G): the type as picture
  // tiles, a plan and a side section with the values on them. The front
  // wall height is entered above the roof under the front, and the depth,
  // ridge and slope lengths follow from the roof's pitch (tectora-core
  // `edit::dormers::section`, mirrored in dormers.ts and parity-tested).
  import { onDestroy } from "svelte";
  import { api } from "./api";
  import { numberLocale, t } from "./i18n.svelte";
  import { materialIds, materialName } from "./materials";
  import { DORMER_KINDS, buildDormer, defaultParams, dormerSection, parseDormer, eaveFor, solveDormer, type DormerKind, type DormerParams, type HostSlope, type SectionProblem, type SectionTarget } from "./dormers";
  import { MAX_ROUND_SEGMENTS } from "./roundroof";
  import { formatNumber, numeric } from "./numeric.svelte";
  import type { Placement, Project } from "./types";
  import PlaceFields from "./PlaceFields.svelte";
  import { notifyError } from "./notify.svelte";

  let {
    project,
    params = $bindable(),
    track = (p: Promise<unknown>) => p,
  }: {
    project: Project;
    /** The dormer being edited; the dialog's OK takes it from here. */
    params: DormerParams;
    /** Registers a placement in flight, so OK waits for it (review UI-001). */
    track?: (p: Promise<unknown>) => Promise<unknown>;
  } = $props();
  // Set once the dialog is gone: a late answer then writes nothing.
  let closed = false;
  onDestroy(() => (closed = true));

  // The roof under the front: only where the front stands matters, so it is
  // asked again only when the start, direction or width change.
  let host = $state<HostSlope | null>(null);
  let hostReady = $state(false);
  let hostFailed = $state(false);
  const hostKey = $derived(`${params.start.x}|${params.start.y}|${params.angle}|${params.width}`);
  let asked = 0;
  $effect(() => {
    void hostKey;
    const n = ++asked;
    const probe = { ...$state.snapshot(project), dormers: [buildDormer($state.snapshot(params) as DormerParams)] } as Project;
    api.dormerHost(probe, 0).then(
      (h) => {
        if (n !== asked) return;
        host = h;
        hostReady = true;
        hostFailed = false;
      },
      () => {
        if (n !== asked) return;
        host = null;
        hostReady = true;
        hostFailed = true;
      },
    );
  });

  // Where the dormer sits on its roof face (from the eave, from the sides),
  // asked of the core for the dormer as it is being edited.
  let placement = $state<Placement | null>(null);
  const placeKey = $derived(JSON.stringify(buildDormer($state.snapshot(params) as DormerParams)));
  let placeAsked = 0;
  $effect(() => {
    void placeKey;
    const n = ++placeAsked;
    api.itemPlacement(probeProject(), "dormer", 0).then(
      (p) => n === placeAsked && (placement = p),
      () => n === placeAsked && (placement = null),
    );
  });
  /** The job with only the dormer being edited (the others do not change where it sits). */
  function probeProject(): Project {
    return { ...$state.snapshot(project), dormers: [buildDormer($state.snapshot(params) as DormerParams)] } as Project;
  }
  /** A distance typed: the core moves the dormer there (MCP place_item) and the dialog takes it back. */
  function place(fromEave: number | null, fromLeft: number | null, fromRight: number | null) {
    track(placeNow(fromEave, fromLeft, fromRight));
  }
  async function placeNow(fromEave: number | null, fromLeft: number | null, fromRight: number | null) {
    const n = ++placeAsked;
    try {
      const next = await api.placeItem(probeProject(), "dormer", 0, fromEave, fromLeft, fromRight);
      const d = next.dormers[0];
      if (!d || n !== placeAsked || closed) return;
      const q = parseDormer(d);
      if (q.kind === "general") return;
      params = { ...params, start: q.start, eave_height: q.eave_height };
    } catch (e) {
      if (!closed) notifyError(e, "calc");
    }
  }

  const sec = $derived(host ? dormerSection(params, host) : null);
  let problem = $state<SectionProblem | null>(null);

  const angleDeg = $derived(+(((params.angle * 180) / Math.PI + 360) % 360).toFixed(2));
  const fronted = (k: DormerKind) => k === "shed" || k === "trapezoid";
  /** A shed dormer, or a trapezoid whose shed roof meets the roof before its cheeks close. */
  const shedTop = $derived(params.kind === "shed" || (params.kind === "trapezoid" && (sec?.back_width ?? 0) > 0));
  const hasRidge = $derived(!shedTop);

  function set(patch: Partial<DormerParams>) {
    const next = { ...params, ...patch };
    if (patch.kind && fronted(patch.kind) && !fronted(params.kind)) {
      next.front_pitch_deg = defaultParams(patch.kind, next.start, next.angle, next.eave_height).front_pitch_deg;
    }
    // A pointed dormer's eave lies on the roof (DACAD: Spitzgaube).
    if (next.kind === "pointed" && host) {
      const e = eaveFor(next, host, { wall_height: 0 });
      if (typeof e === "number") next.eave_height = e;
    }
    problem = null;
    params = next;
  }

  /** Keeps the front wall height when the shape changes, as the dialog does. */
  function setShape(patch: Partial<DormerParams>) {
    const wall = sec?.wall_height;
    set(patch);
    if (host && wall !== undefined && params.kind !== "pointed") {
      const e = eaveFor(params, host, { wall_height: Math.max(0, wall) });
      if (typeof e === "number") params = { ...params, eave_height: e };
    }
  }

  function setBy(target: SectionTarget) {
    if (!host) return;
    const e = eaveFor(params, host, target);
    if (typeof e === "number") {
      problem = null;
      params = { ...params, eave_height: e };
    } else {
      problem = e;
    }
  }

  /** A trapezoid's back width sets its cheek pitch; the front wall keeps its height. */
  function setBackWidth(width: number) {
    if (!host || !sec) return;
    const q = solveDormer(params, host, { wall_height: Math.max(0, sec.wall_height) }, width);
    if (typeof q === "string") {
      problem = q;
    } else {
      problem = null;
      params = q;
    }
  }

  function reset() {
    const d = defaultParams(params.kind, params.start, params.angle, params.eave_height);
    params = { ...d, material: params.material };
    problem = null;
    if (host && params.kind !== "pointed") setBy({ wall_height: 1 });
    else if (host) set({});
  }

  const mm = (v: number | null | undefined) => (v === null || v === undefined ? 0 : Math.round(v * 1000) / 1000);
  const fmt = (v: number | null | undefined, digits = 3) => (v === null || v === undefined ? "–" : formatNumber(v, numberLocale(), digits));

  // Tile pictures: the dormer's front as a roofer draws it, 40 × 30.
  const TILE: Record<string, string> = {
    gable: "M8 28V14L20 4L32 14V28Z",
    pointed: "M4 28L20 6L36 28Z",
    hip: "M8 28V14L20 5L32 14V28ZM8 14H32M20 5L14 14M20 5L26 14",
    half_hip: "M8 28V12L14 6L26 6L32 12V28ZM14 6L20 2L26 6",
    shed: "M8 28V12L32 6V28Z",
    trapezoid: "M8 28V14L14 6H26L32 14V28Z",
    round: "M8 28V16A12 12 0 0 1 32 16V28Z",
  };
</script>

<div class="dlg">
  <div class="kinds" role="radiogroup" aria-label={t("dormer.kind")}>
    {#each DORMER_KINDS as k}
      <button
        class="kind"
        class:on={params.kind === k}
        role="radio"
        aria-checked={params.kind === k}
        onclick={() => setShape({ kind: k })}
      >
        <svg viewBox="0 0 40 30" aria-hidden="true"><path d={TILE[k]} /><line x1="2" y1="28.5" x2="38" y2="28.5" /></svg>
        <span>{t(`dormer.${k}`)}</span>
      </button>
    {/each}
  </div>

  <div class="views">
    <!-- Plan: the front at the bottom, the dormer runs up into the roof. -->
    <figure class="view">
      <figcaption>{t("dormer.plan")}</figcaption>
      <svg viewBox="0 0 300 220" aria-hidden="true">
        <rect class="roof" x="0" y="0" width="300" height="220" />
        {#if params.kind === "shed"}
          <path class="dormer" d="M80 175V45H220V175" />
        {:else}
          <path class="dormer" d="M80 175V45H220V175" />
          {#if params.kind === "trapezoid"}
            <path class="line" d="M80 175L118 45M220 175L182 45" />
            <path class="dim" d="M118 30H182M118 24V36M182 24V36" />
          {:else if params.kind === "hip" || params.kind === "half_hip"}
            <path class="line" d="M80 175L150 125L220 175M150 125V45" />
          {:else}
            <path class="line" d="M150 175V45" />
          {/if}
        {/if}
        <line class="front" x1="80" y1="175" x2="220" y2="175" />
        <path class="dim" d="M80 198H220M80 192V204M220 192V204" />
        <path class="dim" d="M56 175V45M50 175H62M50 45H62" />
        <path class="dim" d="M220 110H244" />
      </svg>
      <label class="f" style="left:50%;top:84%" title={t("dormer.width")}>
        <input aria-label={t("dormer.width")} type="text" use:numeric={{ value: params.width, step: 0.05, min: 0.1, onchange: (v) => setShape({ width: v }) }} /><em>m</em>
      </label>
      <label class="f" style="left:10%;top:50%" title={t("dormer.depth")}>
        <input aria-label={t("dormer.depth")} type="text" disabled={!sec || params.kind === "pointed"} use:numeric={{ value: mm(sec?.depth), step: 0.05, min: 0.01, onchange: (v) => setBy({ depth: v }) }} /><em>m</em>
      </label>
      <label class="f" style="left:88%;top:50%" title={t("dormer.sideOverhang")}>
        <input aria-label={t("dormer.sideOverhang")} type="text" use:numeric={{ value: params.side_overhang, step: 0.05, min: 0, onchange: (v) => setShape({ side_overhang: v }) }} /><em>m</em>
      </label>
      {#if params.kind === "trapezoid"}
        <label class="f" style="left:50%;top:7%" title={t("dormer.backWidth")}>
          <input aria-label={t("dormer.backWidth")} type="text" disabled={!sec || sec.back_width === null} use:numeric={{ value: mm(sec?.back_width), step: 0.05, min: 0.01, onchange: (v) => setBackWidth(v) }} /><em>m</em>
        </label>
      {/if}
      {#if params.kind === "round"}
        <label class="f" style="left:50%;top:22%" title={t("round.segments")}>
          <input aria-label={t("round.segments")} type="text" use:numeric={{ value: params.segments, step: 1, min: 0, max: MAX_ROUND_SEGMENTS, onchange: (v) => setShape({ segments: Math.round(v) }) }} />
        </label>
      {:else}
        <label class="f" style="left:50%;top:{params.kind === "trapezoid" ? 40 : 22}%" title={params.kind === "shed" ? t("dormer.frontPitch") : params.kind === "trapezoid" ? t("dormer.cheekPitch") : t("edge.pitch")}>
          {#if params.kind === "shed"}
            <input aria-label={t("dormer.frontPitch")} type="text" use:numeric={{ value: params.front_pitch_deg, step: 1, min: 0.1, max: 89.9, onchange: (v) => setShape({ front_pitch_deg: v }) }} /><em>°</em>
          {:else}
            <input aria-label={params.kind === "trapezoid" ? t("dormer.cheekPitch") : t("edge.pitch")} type="text" use:numeric={{ value: params.pitch_deg, step: 1, min: 0.1, max: 89.9, onchange: (v) => setShape({ pitch_deg: v }) }} /><em>°</em>
          {/if}
        </label>
      {/if}
      {#if params.kind === "trapezoid"}
        <label class="f" style="left:50%;top:62%" title={t("dormer.frontPitch")}>
          <input aria-label={t("dormer.frontPitch")} type="text" use:numeric={{ value: params.front_pitch_deg, step: 1, min: 0.1, max: 89.9, onchange: (v) => setShape({ front_pitch_deg: v }) }} /><em>°</em>
        </label>
      {/if}
      {#if params.kind === "half_hip"}
        <label class="f" style="left:36%;top:64%" title={t("dormer.halfHipHeight")}>
          <input aria-label={t("dormer.halfHipHeight")} type="text" use:numeric={{ value: params.half_hip_height, step: 0.05, min: 0.05, onchange: (v) => setShape({ half_hip_height: v }) }} /><em>m</em>
        </label>
        <label class="f" style="left:64%;top:64%" title={t("edge.halfHipPitch")}>
          <input aria-label={t("edge.halfHipPitch")} type="text" use:numeric={{ value: params.half_hip_pitch_deg, step: 1, min: 0.1, max: 89.9, onchange: (v) => setShape({ half_hip_pitch_deg: v }) }} /><em>°</em>
        </label>
      {/if}
    </figure>

    <!-- Section: the front on the left, the roof rising to the right. -->
    <figure class="view">
      <figcaption>{t("dormer.sectionView")}</figcaption>
      <svg viewBox="0 0 300 220" aria-hidden="true">
        <line class="host" x1="20" y1="205" x2="280" y2="35" />
        {#if shedTop}
          <line class="front" x1="80" y1="166" x2="80" y2="110" />
          <line class="dormer-top" x1="80" y1="110" x2="250" y2="55" />
          <path class="dim" d="M80 182H250M250 55V182" />
        {:else}
          <line class="front" x1="80" y1="166" x2="80" y2="110" />
          {#if params.kind === "hip" || params.kind === "trapezoid" || params.kind === "half_hip"}
            <path class="dormer-top" d={params.kind === "half_hip" ? "M80 110V96L112 72H230" : "M80 110L112 72H230"} />
          {:else}
            <path class="dormer-top" d="M80 110V72H230" />
          {/if}
          <path class="dim" d="M80 182H230M230 72V182" />
        {/if}
        <path class="dim arc" d="M48 205A30 30 0 0 0 46 186" />
      </svg>
      <label class="f" style="left:13%;top:36%" title={t("dormer.frontOverhang")}>
        <input aria-label={t("dormer.frontOverhang")} type="text" use:numeric={{ value: params.overhang, step: 0.05, min: 0, onchange: (v) => setShape({ overhang: v }) }} /><em>m</em>
      </label>
      <label class="f" style="left:22%;top:63%" title={t("dormer.wallHeight")}>
        <input aria-label={t("dormer.wallHeight")} type="text" disabled={!host || params.kind === "pointed"} use:numeric={{ value: mm(sec?.wall_height), step: 0.05, min: 0, onchange: (v) => setBy({ wall_height: v }) }} /><em>m</em>
      </label>
      {#if hasRidge}
        <label class="f" style="left:56%;top:25%" title={t("dormer.ridgeLength")}>
          <input aria-label={t("dormer.ridgeLength")} type="text" disabled={!sec?.ridge_length && sec?.ridge_length !== 0} use:numeric={{ value: mm(sec?.ridge_length), step: 0.05, min: 0.01, onchange: (v) => setBy({ ridge_length: v }) }} /><em>m</em>
        </label>
      {/if}
      <output class="ro" style="left:55%;top:91%" title={t("dormer.depth")}>{fmt(sec?.depth)} m</output>
      <output class="ro" style="left:{hasRidge ? 80 : 88}%;top:58%" title={t("dormer.topHeight")}>{fmt(sec?.top_height)} m</output>
      <output class="ro" style="left:{hasRidge ? 52 : 62}%;top:{hasRidge ? 62 : 52}%" title={t("dormer.hostLength")}>{fmt(sec?.host_length)} m</output>
      <output class="ro" style="left:20%;top:88%" title={t("dormer.hostPitch")}>{fmt(sec?.host_pitch_deg, 2)}°</output>
    </figure>
  </div>

  <dl class="facts">
    <div><dt>{t("dormer.hostPitch")}</dt><dd>{fmt(sec?.host_pitch_deg, 2)}°</dd></div>
    <div><dt>{t("dormer.depth")}</dt><dd>{fmt(sec?.depth)} m</dd></div>
    {#if hasRidge}<div><dt>{t("dormer.ridgeLength")}</dt><dd>{fmt(sec?.ridge_length)} m</dd></div>{/if}
    {#if params.kind === "trapezoid"}<div><dt>{t("dormer.backWidth")}</dt><dd>{fmt(sec?.back_width)} m</dd></div>{/if}
    <div><dt>{shedTop ? t("dormer.roofLength") : t("dormer.cheekLength")}</dt><dd>{fmt(params.kind === "round" ? null : sec?.roof_length)} m</dd></div>
    <div><dt>{t("dormer.hostLength")}</dt><dd>{fmt(sec?.host_length)} m</dd></div>
    <div><dt>{t("dormer.topHeight")}</dt><dd>{fmt(sec?.top_height)} m</dd></div>
  </dl>

  {#if hostReady && !host}
    <p class="note" role="status"><i></i>{t("dormer.offRoof")}</p>
  {:else if sec && sec.depth === null}
    <p class="note" role="status"><i></i>{t("dormer.noDepth")}</p>
  {/if}
  {#if problem}
    <p class="note" role="alert"><i></i><code>{problem}</code> {t(`dormer.err.${problem}`)}</p>
  {/if}

  <div class="row place-row">
    <span class="row-title">{t("place.title")}</span>
    {#if placement}
      <PlaceFields {placement} onplace={place} width={116} />
    {:else if hostReady}
      <span class="muted">{t("place.offRoof")}</span>
    {/if}
  </div>

  <div class="row">
    <label>
      <span>{t("dormer.eaveAbs")}</span>
      <div class="unit"><input type="text" disabled={params.kind === "pointed" && !!host} use:numeric={{ value: params.eave_height, step: 0.05, onchange: (v) => set({ eave_height: v }) }} /><em>m</em></div>
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
    <label>
      <span>{t("edge.material")}</span>
      <select value={params.material} onchange={(e) => set({ material: +(e.target as HTMLSelectElement).value })}>
        {#each materialIds(project) as m}
          <option value={m}>{materialName(project, m)}</option>
        {/each}
      </select>
    </label>
    <div class="spacer"></div>
    <button class="btn" onclick={reset}>{t("dormer.reset")}</button>
  </div>
</div>

<style>
  .dlg {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .kinds {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 6px;
  }
  .kind {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 6px 4px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 11.5px;
    line-height: 1.2;
    text-align: center;
    cursor: pointer;
  }
  .kind svg {
    width: 40px;
    height: 30px;
  }
  .kind path {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linejoin: round;
  }
  .kind line {
    stroke: var(--text-3);
  }
  .kind.on {
    border-color: var(--accent);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .kind.on path {
    stroke: var(--accent);
  }
  .views {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .view {
    position: relative;
    margin: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    aspect-ratio: 300 / 220;
  }
  .view svg {
    width: 100%;
    height: 100%;
    display: block;
  }
  figcaption {
    position: absolute;
    left: 8px;
    top: 6px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .roof {
    fill: none;
  }
  .dormer,
  .front,
  .dormer-top {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
  }
  .front {
    stroke: var(--text);
  }
  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.2;
    stroke-dasharray: 5 3;
  }
  .host {
    stroke: var(--text-2);
    stroke-width: 1.6;
  }
  .dim {
    fill: none;
    stroke: var(--text-3);
    stroke-width: 1;
  }
  .f,
  .ro {
    position: absolute;
    transform: translate(-50%, -50%);
  }
  /* Wide enough for five digits with the ▲▼ buttons ("4,928", "12,345"). */
  .f input {
    width: 108px;
    height: 32px;
    padding: 0 22px 0 8px;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }
  .f em {
    position: absolute;
    right: 8px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 12px;
  }
  .ro {
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
    white-space: nowrap;
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 18px;
    margin: 0;
    font-size: 13.5px;
  }
  .facts div {
    display: flex;
    gap: 6px;
  }
  .facts dt {
    color: var(--text-3);
  }
  .facts dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
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
    background: var(--text-3);
  }
  .note[role="alert"] i {
    background: var(--danger);
  }
  .note code {
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 12px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 12px;
  }
  .row label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .unit {
    position: relative;
    display: inline-block;
  }
  .unit input {
    width: 116px;
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
  select {
    height: 34px;
  }
  .spacer {
    flex: 1;
  }
  .place-row {
    padding-top: 2px;
  }
  .row-title {
    align-self: center;
    min-width: 120px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-2);
  }
  .muted {
    align-self: center;
    font-size: 12.5px;
    color: var(--text-3);
  }
  @media (max-width: 720px) {
    .kinds {
      grid-template-columns: repeat(4, 1fr);
    }
    .views {
      grid-template-columns: 1fr;
    }
  }
</style>
