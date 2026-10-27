<script lang="ts">
  // The selected chimney, roof window or roof opening, docked like the wall bar.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import type { Chimney, Project, RoofWindow } from "./types";
  import { chimneyFromRect, openingList, rectFromChimney, type ChimneyRect, type OpeningSel } from "./openings";

  let {
    project,
    sel = $bindable(),
    onchimney,
    onwindow,
    ondelete,
  }: {
    project: Project;
    sel: OpeningSel;
    onchimney: (c: Chimney) => void;
    onwindow: (w: RoofWindow) => void;
    ondelete: () => void;
  } = $props();

  const list = $derived(sel ? openingList(project, sel.kind) : []);
  // A roof opening has a chimney's shape, without a top.
  const chimney = $derived(sel && sel.kind !== "window" ? (list[sel.index] as Chimney | undefined) ?? null : null);
  const rect = $derived(chimney ? rectFromChimney(chimney) : null);
  const win = $derived(sel?.kind === "window" ? (project.windows ?? [])[sel.index] : null);

  function num(e: Event): number | null {
    const v = parseFloat((e.target as HTMLInputElement).value.replace(",", "."));
    return Number.isFinite(v) ? v : null;
  }

  function setRect(patch: Partial<ChimneyRect>) {
    if (rect) onchimney(chimneyFromRect({ ...rect, ...patch }));
  }

  function step(d: number) {
    if (!sel || list.length === 0) return;
    sel = { kind: sel.kind, index: (sel.index + d + list.length) % list.length };
  }
</script>

{#snippet field(label: string, value: number, unit: string, set: (v: number) => void, min: number | null = null)}
  <label>
    <span>{label}</span>
    <div class="unit"><input type="number" step="0.05" min={min ?? undefined} {value}
      onchange={(e) => { const v = num(e); if (v !== null && (min === null || v >= min)) set(v); }} /><em>{unit}</em></div>
  </label>
{/snippet}

<div class="bar" role="toolbar" aria-label={t(`opening.${sel?.kind ?? "chimney"}`)}>
  {#if sel}
    <div class="nav">
      <button class="icon-btn" onclick={() => step(-1)} aria-label={t("opening.prev")}><Icon name="left" /></button>
      <div class="which">
        <strong>{t(`opening.${sel.kind}N`, { n: sel.index + 1 })}</strong>
        <span>{t("def.of", { n: list.length })}</span>
      </div>
      <button class="icon-btn" onclick={() => step(1)} aria-label={t("opening.next")}><Icon name="right" /></button>
    </div>
    {#if chimney}
      {#if rect}
        {@render field("X", rect.x, "m", (v) => setRect({ x: v }))}
        {@render field("Y", rect.y, "m", (v) => setRect({ y: v }))}
        {@render field(t("opening.width"), rect.width, "m", (v) => setRect({ width: v }), 0.05)}
        {@render field(t("opening.depth"), rect.depth, "m", (v) => setRect({ depth: v }), 0.05)}
        {@render field(t("opening.rotation"), rect.rotation, "°", (v) => setRect({ rotation: v }))}
      {:else}
        <p class="tip">{t("opening.polygon", { n: chimney.corners.length })}</p>
      {/if}
      {#if sel.kind === "chimney"}
        {@render field(t("opening.top"), chimney.height, "m", (v) => onchimney({ ...$state.snapshot(chimney), height: v }))}
      {:else}
        <p class="tip">{t("opening.holeHint")}</p>
      {/if}
    {:else if win}
      {@render field("X", win.x, "m", (v) => onwindow({ ...win, x: v }))}
      {@render field("Y", win.y, "m", (v) => onwindow({ ...win, y: v }))}
      {@render field(t("opening.width"), win.width, "m", (v) => onwindow({ ...win, width: v }), 0.05)}
      {@render field(t("opening.height"), win.height, "m", (v) => onwindow({ ...win, height: v }), 0.05)}
      <p class="tip">{t("opening.windowHint")}</p>
    {/if}
    <div class="spacer"></div>
    <button class="icon-btn danger" onclick={ondelete} title={t("opening.delete")} aria-label={t("opening.delete")}><Icon name="trash" /></button>
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
  .tip {
    align-self: center;
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
</style>
