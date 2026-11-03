<script lang="ts">
  // DACAD's edit functions for a whole part (Verschieben, Drehen, Spiegeln,
  // Elemente kopieren, Kopieren-Drehen, Kopieren-Spiegeln) and for one wall
  // (Parallele Linie): numbers in, the plan map out.
  import { untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import type { RoofPart } from "./types";
  import { isMarker } from "./types";
  import { mirror, partCentre, rotation, translation, type TransformMode, type TransformOp } from "./transform";

  let {
    part,
    edge,
    mode = $bindable("move"),
    op = $bindable(null),
  }: { part: RoofPart; edge: number | null; mode: TransformMode; op: TransformOp | null } = $props();

  // The pivot starts at the part's centre when the dialog opens.
  const c = untrack(() => partCentre(part));
  let dx = $state(0);
  let dy = $state(0);
  let angle = $state(90);
  let cx = $state(c.x);
  let cy = $state(c.y);
  let axis = $state(90);
  let copies = $state(0);
  let dist = $state(0.5);

  const wallOk = $derived(edge !== null && !!part.edges[edge] && !isMarker(part.edges[edge]));
  const MODES: TransformMode[] = ["move", "rotate", "mirror", "wall"];

  $effect(() => {
    const n = Math.max(0, Math.min(50, Math.round(copies || 0)));
    const fin = (...v: number[]) => v.every((x) => Number.isFinite(x));
    if (mode === "move") op = fin(dx, dy) && (dx || dy) ? { kind: "map", map: translation(dx, dy), copies: n } : null;
    else if (mode === "rotate") op = fin(angle, cx, cy) && angle % 360 ? { kind: "map", map: rotation({ x: cx, y: cy }, angle), copies: n } : null;
    else if (mode === "mirror") op = fin(axis, cx, cy) ? { kind: "map", map: mirror({ x: cx, y: cy }, axis), copies: Math.min(n, 1) } : null;
    else op = wallOk && fin(dist) && dist ? { kind: "wall", edge: edge!, dist } : null;
  });
</script>

<div class="form">
  <div class="seg" role="radiogroup" aria-label={t("xf.title")}>
    {#each MODES as m}
      <button class:on={mode === m} role="radio" aria-checked={mode === m} disabled={m === "wall" && !wallOk} onclick={() => (mode = m)}>{t(`xf.${m}`)}</button>
    {/each}
  </div>
  <p class="hint">{t(`xf.${mode}Hint`)}</p>
  <div class="fields">
    {#if mode === "move"}
      <label><span>{t("xf.dx")}</span><div class="unit"><input type="number" step="0.1" bind:value={dx} /><em>m</em></div></label>
      <label><span>{t("xf.dy")}</span><div class="unit"><input type="number" step="0.1" bind:value={dy} /><em>m</em></div></label>
    {:else if mode === "rotate" || mode === "mirror"}
      {#if mode === "rotate"}
        <label><span>{t("xf.angle")}</span><div class="unit"><input type="number" step="1" bind:value={angle} /><em>°</em></div></label>
      {:else}
        <label><span>{t("xf.axis")}</span><div class="unit"><input type="number" step="1" bind:value={axis} /><em>°</em></div></label>
      {/if}
      <label><span>{t("xf.cx")}</span><div class="unit"><input type="number" step="0.1" bind:value={cx} /><em>m</em></div></label>
      <label><span>{t("xf.cy")}</span><div class="unit"><input type="number" step="0.1" bind:value={cy} /><em>m</em></div></label>
    {:else}
      <label><span>{t("xf.dist")}</span><div class="unit"><input type="number" step="0.05" bind:value={dist} /><em>m</em></div></label>
    {/if}
    {#if mode !== "wall"}
      <label title={t("xf.copiesHint")}>
        <span>{t("xf.copies")}</span>
        <input type="number" step="1" min="0" max={mode === "mirror" ? 1 : 50} bind:value={copies} />
      </label>
    {/if}
  </div>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  .seg button {
    flex: 1;
    padding: 7px 8px;
    border: 0;
    background: transparent;
    color: var(--text);
    cursor: pointer;
  }
  .seg button + button {
    border-left: 1px solid var(--border);
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .seg button:disabled {
    color: var(--text-3);
    cursor: default;
  }
  .hint {
    margin: 0;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  .unit {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .unit input,
  label > input {
    width: 100%;
  }
  .unit em {
    font-style: normal;
    color: var(--text-2);
    font-size: 12px;
  }
</style>
