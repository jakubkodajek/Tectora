<script lang="ts">
  // The selected chimney, roof window or roof opening, docked like the wall bar.
  import Icon from "./Icon.svelte";
  import { freshBar } from "./dblclick";
  import { t } from "./i18n.svelte";
  import type { Chimney, Placement, Project, RoofWindow } from "./types";
  import { isConvex, openingBox, openingList, type OpeningBox, type OpeningSel } from "./openings";
  import { api } from "./api";
  import { notifyError } from "./notify.svelte";
  import type { BarEdit } from "./jobedits";
  import { numeric } from "./numeric.svelte";
  import PlaceFields from "./PlaceFields.svelte";
  import { pageStep, stepIndex } from "./itemstep";

  let {
    project,
    sel = $bindable(),
    onchimney,
    onwindow,
    ondelete,
    placement = null,
    onplace,
    edit,
  }: {
    project: Project;
    sel: OpeningSel;
    /** Stores `c` as the chimney or opening `at` (the one the value was typed for). */
    onchimney: (c: Chimney, at: NonNullable<OpeningSel>) => void;
    /** A field of roof window `at` typed; resolves to the reason when it was not stored. */
    onwindow: (patch: Partial<RoofWindow>, at: number) => unknown;
    ondelete: () => void;
    /** Where the object sits on its face (dimension fields). */
    placement?: Placement | null;
    onplace?: (fromEave: number | null, fromLeft: number | null, fromRight: number | null) => unknown;
    /** Queues a core-backed edit of item `index` of `list`, held now (the editor's `barEdit`, jobedits.ts). */
    edit: BarEdit;
  } = $props();

  const list = $derived(sel ? openingList(project, sel.kind) : []);
  // A roof opening has a chimney's shape, without a top.
  const chimney = $derived(sel && sel.kind !== "window" ? (list[sel.index] as Chimney | undefined) ?? null : null);
  const box = $derived(chimney ? openingBox(chimney.corners) : null);
  const win = $derived(sel?.kind === "window" ? (project.windows ?? [])[sel.index] : null);
  /** The item the fields edit now: a field reused for the next one forgets the one before (UI-403). */
  const itemKey = $derived(sel ? `${sel.kind}:${sel.index}` : "");


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
</script>

{#snippet field(label: string, value: number, unit: string, set: (v: number) => unknown, min: number | null = null)}
  <label>
    <span>{label}</span>
    <div class="unit"><input type="text" use:numeric={{ key: itemKey, value, step: 0.05, min: min ?? undefined, onchange: set }} /><em>{unit}</em></div>
  </label>
{/snippet}

<div class="bar" role="toolbar" aria-label={t(`opening.${sel?.kind ?? "chimney"}`)} use:pageStep={step} use:freshBar>
  {#if sel}
    <div class="nav">
      <button class="icon-btn" onclick={() => step(-1)} title="{t('opening.prev')} (PageUp)" aria-label={t("opening.prev")}><Icon name="left" /></button>
      <div class="which">
        <strong>{t(`opening.${sel.kind}N`, { n: sel.index + 1 })}</strong>
        <span>{t("def.of", { n: list.length })}</span>
      </div>
      <button class="icon-btn" onclick={() => step(1)} title="{t('opening.next')} (PageDown)" aria-label={t("opening.next")}><Icon name="right" /></button>
    </div>
    {#if chimney}
      {#if box}
        {@render field("X", box.x, "m", (v) => setBox({ x: v }))}
        {@render field("Y", box.y, "m", (v) => setBox({ y: v }))}
        {@render field(t("opening.width"), box.width, "m", (v) => setBox({ width: v }), 0.05)}
        {@render field(t("opening.depth"), box.depth, "m", (v) => setBox({ depth: v }), 0.05)}
        {@render field(t("opening.rotation"), +box.rotation.toFixed(1), "°", (v) => setBox({ rotation: v }))}
        {#if chimney.corners.length !== 4}
          <p class="tip">{t("opening.polygon", { n: chimney.corners.length })}</p>
        {/if}
      {:else}
        <p class="tip warn">{t("opening.noArea")}</p>
      {/if}
      {#if !isConvex(chimney.corners)}
        <p class="tip warn">{t("opening.nonConvex")}</p>
      {/if}
      {#if sel.kind === "chimney"}
        {@render field(t("opening.top"), chimney.height, "m", setTop)}
      {:else}
        <p class="tip">{t("opening.holeHint")}</p>
      {/if}
    {:else if win}
      {@render field("X", win.x, "m", (v) => sel && onwindow({ x: v }, sel.index))}
      {@render field("Y", win.y, "m", (v) => sel && onwindow({ y: v }, sel.index))}
      {@render field(t("opening.width"), win.width, "m", (v) => sel && onwindow({ width: v }, sel.index), 0.05)}
      {@render field(t("opening.height"), win.height, "m", (v) => sel && onwindow({ height: v }, sel.index), 0.05)}
      <p class="tip">{t("opening.windowHint")}</p>
    {/if}
    {#if onplace}<PlaceFields {placement} {onplace} item={itemKey} />{/if}
    <div class="spacer"></div>
    {@const del = t(sel.kind === "chimney" ? "opening.deleteChimney" : sel.kind === "window" ? "opening.deleteWindow" : "opening.deleteHole")}
    <button class="btn sm ghost danger" onclick={ondelete} title={del}><Icon name="trash" size={15} />{del}</button>
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
  .tip.warn {
    color: var(--danger);
    font-weight: 600;
  }
  .tip {
    align-self: center;
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
</style>
