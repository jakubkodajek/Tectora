<script lang="ts">
  // The selected chimney, roof window or roof opening: docked like the wall
  // bar, or as the body of its Properties dialog (`dialog`). Its fields sit in
  // three named groups, always in this order: the object's size, its position
  // on the roof face (distances from the eave and both edges) and its
  // position in plan (a corner and the rotation). Read-only facts say what
  // the numbers refer to; the dialog adds a sketch of the roof face.
  import Icon from "./Icon.svelte";
  import { freshBar } from "./dblclick";
  import { numberLocale, t } from "./i18n.svelte";
  import type { CalcResult, Chimney, Placement, Point2, Project, RoofWindow } from "./types";
  import { isConvex, openingBox, openingList, windowOutline, type OpeningBox, type OpeningSel } from "./openings";
  import { api } from "./api";
  import { notifyError } from "./notify.svelte";
  import type { BarEdit } from "./jobedits";
  import { formatNumber, numeric } from "./numeric.svelte";
  import PlaceFields, { type PlaceKey } from "./PlaceFields.svelte";
  import PlaceSketch from "./PlaceSketch.svelte";
  import { pageStep, stepIndex } from "./itemstep";
  import { hostHeightAt } from "./dormers";
  import { maxOf, minOf } from "./extent";
  import { prefs, savePrefs } from "./settings.svelte";

  let {
    project,
    result = null,
    sel = $bindable(),
    onchimney,
    onwindow,
    ondelete,
    onedit,
    placement = null,
    onplace,
    edit,
    dialog = false,
  }: {
    project: Project;
    /** The calculated roof: the facts (height above the roof) and the sketch. */
    result?: CalcResult | null;
    sel: OpeningSel;
    /** Stores `c` as the chimney or opening `at` (the one the value was typed for). */
    onchimney: (c: Chimney, at: NonNullable<OpeningSel>) => void;
    /** A field of roof window `at` typed; resolves to the reason when it was not stored. */
    onwindow: (patch: Partial<RoofWindow>, at: number) => unknown;
    ondelete: () => void;
    /** Opens the Properties dialog (docked bar only). */
    onedit?: () => void;
    /** Where the object sits on its face (dimension fields). */
    placement?: Placement | null;
    onplace?: (fromEave: number | null, fromLeft: number | null, fromRight: number | null) => unknown;
    /** Queues a core-backed edit of item `index` of `list`, held now (the editor's `barEdit`, jobedits.ts). */
    edit: BarEdit;
    /** The body of the Properties dialog: every group open, the sketch, no delete (it is in the footer). */
    dialog?: boolean;
  } = $props();

  const list = $derived(sel ? openingList(project, sel.kind) : []);
  // A roof opening has a chimney's shape, without a top.
  const chimney = $derived(sel && sel.kind !== "window" ? (list[sel.index] as Chimney | undefined) ?? null : null);
  const box = $derived(chimney ? openingBox(chimney.corners) : null);
  const win = $derived(sel?.kind === "window" ? (project.windows ?? [])[sel.index] : null);
  /** The item the fields edit now: a field reused for the next one forgets the one before (UI-403). */
  const itemKey = $derived(sel ? `${sel.kind}:${sel.index}` : "");
  /** The distance field with the focus, for the sketch. */
  let active = $state<PlaceKey | null>(null);

  /**
   * A field of the box typed: the core reshapes the corners; a box it
   * refuses is said at the field. The value belongs to the object selected
   * now (held, so ▶ or PageDown does not move it: UI-202), edits run one
   * after another on the object as the one before left it, and only the
   * field typed is the user's (UI-301).
   */
  const setBox = (patch: Partial<OpeningBox>) => {
    if (!sel || sel.kind === "window" || !chimney || !box) return;
    const kind = sel.kind;
    return edit(
      kind === "chimney" ? "chimneys" : "roof_openings",
      sel.index,
      async (job, at) => {
        const cur = $state.snapshot(openingList(job, kind)[at]) as Chimney;
        const now = openingBox(cur.corners);
        if (!now) throw { code: "refused" };
        return { ...cur, corners: await api.reshapeOpening(cur.corners, { ...now, ...patch }) };
      },
      (c, at) => onchimney(c, { kind, index: at }),
    ).catch((e) => {
      if ((e as { code?: string } | null)?.code !== "refused") notifyError(e, "calc");
      return t("num.refused");
    });
  };

  /** The chimney's top typed: stored on the chimney it was typed for. */
  function setTop(v: number) {
    if (!sel || !chimney) return;
    onchimney({ ...$state.snapshot(chimney), height: v }, { kind: sel.kind, index: sel.index } as NonNullable<OpeningSel>);
  }

  function step(d: number) {
    const next = sel ? stepIndex(sel.index, list.length, d) : null;
    if (sel && next !== null) sel = { kind: sel.kind, index: next };
  }

  /** Position in plan and the facts sit in the bar's "More" row, opened per user (like the dormer bar). */
  const moreOpen = $derived(dialog || !!prefs.objectMore);
  function toggleMore() {
    prefs.objectMore = !prefs.objectMore;
    savePrefs();
  }

  const fmt = (v: number, digits: number) => formatNumber(v, numberLocale(), digits);

  /** Plan area of a polygon (shoelace), m². */
  function planArea(k: Point2[]): number {
    let a = 0;
    for (let i = 0; i < k.length; i++) {
      const p = k[i], q = k[(i + 1) % k.length];
      a += p.x * q.y - q.x * p.y;
    }
    return Math.abs(a) / 2;
  }

  /**
   * How far the chimney's top stands above the roof on its upper and lower
   * side: the top minus the roof height under its corners (each just inside
   * the chimney), as the dormer's host height. Null off the roof.
   */
  const above = $derived.by(() => {
    if (!chimney || sel?.kind !== "chimney" || chimney.corners.length < 3) return null;
    const n = chimney.corners.length;
    const c = { x: chimney.corners.reduce((s, p) => s + p.x, 0) / n, y: chimney.corners.reduce((s, p) => s + p.y, 0) / n };
    const zs: number[] = [];
    for (const p of chimney.corners) {
      const d = Math.hypot(c.x - p.x, c.y - p.y);
      const k = d > 1e-6 ? 1e-3 / d : 0;
      const z = hostHeightAt(result, { x: p.x + (c.x - p.x) * k, y: p.y + (c.y - p.y) * k });
      if (z === null || !Number.isFinite(z)) return null;
      zs.push(z);
    }
    return { up: chimney.height - maxOf(zs), down: chimney.height - minOf(zs) };
  });

  /** The sketch's outline of the object. */
  const outline = $derived(chimney ? chimney.corners : win ? windowOutline(result, win) : []);

  /** Facts about where the object sits. */
  const faceFact = $derived(
    placement
      ? t("place.face", { part: t("editor.partN", { n: placement.part + 1 }), n: placement.edge + 1, p: fmt(placement.pitch_deg, 1).replace(/[.,]0$/, "") })
      : null,
  );

  const del = $derived(t(sel?.kind === "chimney" ? "opening.deleteChimney" : sel?.kind === "window" ? "opening.deleteWindow" : "opening.deleteHole"));
</script>

{#snippet field(label: string, value: number, unit: string, set: (v: number) => unknown, opts: { min?: number; title?: string; testid?: string } = {})}
  <label title={opts.title}>
    <span>{label}</span>
    <div class="unit"><input type="text" data-testid={opts.testid} use:numeric={{ key: itemKey, value, step: 0.05, min: opts.min, onchange: set }} /><em>{unit}</em></div>
  </label>
{/snippet}

{#snippet factsOfSize()}
  {#if chimney && sel?.kind === "chimney" && above}
    <p class="obj-fact" data-testid="object-above">{t("opening.aboveRoof", { up: fmt(above.up, 2), down: fmt(above.down, 2) })}</p>
  {/if}
  {#if chimney && box}
    <p class="obj-fact">{t("opening.planArea", { a: fmt(planArea(chimney.corners), 2) })}</p>
  {/if}
  {#if win}
    <p class="obj-fact">{t("opening.windowArea", { a: fmt(win.width * win.height, 2) })}</p>
    {#if placement}<p class="obj-fact">{t("opening.windowPlan", { h: fmt(win.height * Math.cos((placement.pitch_deg * Math.PI) / 180), 3) })}</p>{/if}
  {/if}
  {#if sel?.kind === "hole"}<p class="obj-fact">{t("opening.holeHint")}</p>{/if}
{/snippet}

{#snippet factsOfPlace()}
  {#if placement && faceFact}
    <p class="obj-fact" data-testid="object-face">{faceFact}</p>
    <p class="obj-fact">{t("place.eavePlan", { v: fmt(placement.from_eave_plan, 3) })}</p>
  {/if}
{/snippet}

{#snippet sizeGroup()}
  <section class="obj-grp" role="group" aria-labelledby="obj-size-{dialog ? 'd' : 'b'}">
    <span class="obj-cap" id="obj-size-{dialog ? 'd' : 'b'}">{t("opening.size")}</span>
    <div class="fields">
      {#if chimney}
        {#if box}
          {@render field(t("opening.width"), box.width, "m", (v) => setBox({ width: v }), { min: 0.05 })}
          {@render field(t("opening.depth"), box.depth, "m", (v) => setBox({ depth: v }), { min: 0.05 })}
        {/if}
        {#if sel?.kind === "chimney"}
          {@render field(t("opening.top"), chimney.height, "m", setTop, { title: t("opening.topHint") })}
        {/if}
      {:else if win}
        {@render field(t("opening.width"), win.width, "m", (v) => sel && onwindow({ width: v }, sel.index), { min: 0.05 })}
        {@render field(t("opening.height"), win.height, "m", (v) => sel && onwindow({ height: v }, sel.index), { min: 0.05, title: t("opening.windowHint") })}
      {/if}
    </div>
    {#if chimney}
      {#if !box}<p class="obj-note" role="status">{t("opening.noArea")}</p>
      {:else if chimney.corners.length !== 4}<p class="obj-note info" role="status">{t("opening.polygon", { n: chimney.corners.length })}</p>{/if}
      {#if !isConvex(chimney.corners)}<p class="obj-note" role="status">{t("opening.nonConvex")}</p>{/if}
      {#if above && above.up < 0}<p class="obj-note" role="status" data-testid="object-top-below">{t("opening.topBelow")}</p>{/if}
    {/if}
    {#if dialog}{@render factsOfSize()}{/if}
  </section>
{/snippet}

{#snippet roofGroup()}
  <section class="obj-grp" role="group" aria-labelledby="obj-roof-{dialog ? 'd' : 'b'}" title={dialog ? undefined : t("place.how")}>
    <span class="obj-cap" id="obj-roof-{dialog ? 'd' : 'b'}">{t("place.title")}</span>
    {#if placement && onplace}
      <div class="fields"><PlaceFields {placement} {onplace} item={itemKey} width={dialog ? 108 : 92} onactive={(k) => (active = k)} /></div>
      {#if dialog}
        {@render factsOfPlace()}
        <p class="obj-hint">{t("place.how")}</p>
      {/if}
    {:else}
      <p class="obj-note" role="status" data-testid="object-off-roof">{t("place.offRoof")}</p>
    {/if}
  </section>
{/snippet}

{#snippet planGroup()}
  <section class="obj-grp" role="group" aria-labelledby="obj-plan-{dialog ? 'd' : 'b'}">
    <span class="obj-cap" id="obj-plan-{dialog ? 'd' : 'b'}">{t("place.plan")}</span>
    <div class="fields">
      {#if chimney && box}
        {@render field(t("place.cornerX"), box.x, "m", (v) => setBox({ x: v }), { title: t("opening.cornerHint") })}
        {@render field(t("place.cornerY"), box.y, "m", (v) => setBox({ y: v }), { title: t("opening.cornerHint") })}
        {@render field(t("opening.rotation"), +box.rotation.toFixed(1), "°", (v) => setBox({ rotation: v }), { title: t("opening.rotationHint") })}
      {:else if win}
        {@render field(t("place.cornerX"), win.x, "m", (v) => sel && onwindow({ x: v }, sel.index), { title: t("opening.windowCornerHint") })}
        {@render field(t("place.cornerY"), win.y, "m", (v) => sel && onwindow({ y: v }, sel.index), { title: t("opening.windowCornerHint") })}
      {/if}
    </div>
    {#if dialog}<p class="obj-hint">{t(win ? "opening.windowCornerHint" : "opening.cornerHint")}</p>{/if}
  </section>
{/snippet}

{#snippet stepper(compact: boolean)}
  <div class="nav" class:compact>
    <button class="icon-btn" onclick={() => step(-1)} title="{t('opening.prev')} (PageUp)" aria-label={t("opening.prev")}><Icon name="left" /></button>
    <div class="which">
      {#if sel}
        {#if compact}
          <span>{sel.index + 1} {t("def.of", { n: list.length })}</span>
        {:else}
          <strong>{t(`opening.${sel.kind}N`, { n: sel.index + 1 })}</strong>
          <span>{t("def.of", { n: list.length })}</span>
        {/if}
      {/if}
    </div>
    <button class="icon-btn" onclick={() => step(1)} title="{t('opening.next')} (PageDown)" aria-label={t("opening.next")}><Icon name="right" /></button>
  </div>
{/snippet}

{#if dialog}
  <div class="dlg" role="group" aria-label={t(`opening.${sel?.kind ?? "chimney"}`)} use:pageStep={step} data-testid="object-editor">
    {#if sel}
      {#if list.length > 1}{@render stepper(true)}{/if}
      <div class="dlg-grid">
        <div class="groups">
          {@render sizeGroup()}
          {@render roofGroup()}
          {@render planGroup()}
        </div>
        <PlaceSketch {result} {placement} {outline} kind={sel.kind} {active} />
      </div>
    {/if}
  </div>
{:else}
  <div class="bar" role="toolbar" aria-label={t(`opening.${sel?.kind ?? "chimney"}`)} use:pageStep={step} use:freshBar>
    {#if sel}
      <div class="line">
        <div class="groups">
          {@render stepper(false)}
          {@render sizeGroup()}
          {@render roofGroup()}
        </div>
        <div class="end">
          {#if onedit}
            <button class="btn sm edit" onclick={onedit} title={t("opening.propertiesHint")} aria-label={t("menu.properties")} data-testid="object-properties"><Icon name="draw" size={15} /><span class="label">{t("menu.properties")}</span></button>
          {/if}
          <button class="btn sm more-btn" class:on={moreOpen} data-testid="object-more" aria-expanded={moreOpen} aria-controls="object-more-row" title={t("opening.moreHint")} onclick={toggleMore}>{t("bar.more")}</button>
          <button class="btn sm ghost danger del" onclick={ondelete} title={del} aria-label={del}><Icon name="trash" size={15} /><span class="label">{del}</span></button>
        </div>
      </div>
      {#if moreOpen}
        <div class="more" id="object-more-row">
          {@render planGroup()}
          <div class="facts">
            {@render factsOfSize()}
            {@render factsOfPlace()}
          </div>
        </div>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .bar {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px 12px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    min-height: 70px;
    container-type: inline-size;
  }
  .line {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .bar .groups {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px 22px;
  }
  .bar .obj-grp .obj-cap {
    margin-bottom: 3px;
    font-size: 11px;
  }
  .fields {
    display: flex;
    align-items: flex-end;
    gap: 10px 10px;
  }
  .bar .fields {
    flex-wrap: nowrap;
  }
  .dlg .fields {
    flex-wrap: wrap;
    gap: 10px 12px;
  }
  .end {
    flex-shrink: 0;
    display: flex;
    gap: 6px;
    margin-top: 18px;
  }
  .edit,
  .del {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .more-btn.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  /* Narrow (the 600 px window): the stepper is one short line beside the buttons. */
  @container (max-width: 700px) {
    .bar .nav {
      border-right: 0;
      padding-right: 0;
    }
    .bar .which {
      flex-direction: row;
      align-items: baseline;
      gap: 6px;
      min-width: 0;
    }
    .end {
      margin-top: 0;
    }
  }
  @container (max-width: 1180px) {
    .del .label,
    .edit .label {
      display: none;
    }
  }
  .more {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 8px 28px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .more .facts {
    max-width: 440px;
    padding-top: 12px;
  }
  .more .facts :global(.obj-fact:first-child) {
    margin-top: 0;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 14px;
    border-right: 1px solid var(--border);
    align-self: stretch;
  }
  .nav.compact {
    border-right: 0;
    padding: 0;
    margin: -4px 0 12px -6px;
  }
  .which {
    display: flex;
    flex-direction: column;
    min-width: 92px;
    line-height: 1.25;
  }
  .compact .which {
    min-width: 0;
    padding: 0 4px;
    font-size: 13px;
    color: var(--text-2);
  }
  .which strong {
    font-size: 14px;
  }
  .which span {
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .compact .which span {
    font-size: 13px;
    color: var(--text-2);
  }
  label span {
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
    padding-left: 8px;
    font-variant-numeric: tabular-nums;
  }
  .dlg .unit input {
    width: 108px;
  }
  .unit em {
    position: absolute;
    right: 9px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
    pointer-events: none;
  }
  .danger {
    color: var(--danger);
  }
  .dlg {
    container-type: inline-size;
  }
  .dlg-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 24px;
    align-items: start;
  }
  .dlg .groups {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .dlg-grid > :global(.sketch) {
    position: sticky;
    top: 0;
  }
  @container (max-width: 620px) {
    .dlg-grid {
      grid-template-columns: minmax(0, 1fr);
    }
    .dlg-grid > :global(.sketch) {
      position: relative;
      order: -1;
      max-width: 360px;
    }
  }
</style>
