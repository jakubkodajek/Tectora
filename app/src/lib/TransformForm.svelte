<script lang="ts">
  // DACAD's edit functions for a whole part (Verschieben, Drehen, Spiegeln,
  // Elemente kopieren, Kopieren-Drehen, Kopieren-Spiegeln) and for one wall
  // (Parallele Linie): numbers in, the plan map out.
  import { untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import { radiogroup } from "./tabs";
  import type { RoofPart } from "./types";
  import { isMarker } from "./types";
  import { api } from "./api";
  import { notifyError } from "./notify.svelte";
  import type { TransformMode, TransformOp } from "./transform";
  import { numeric } from "./numeric.svelte";
  import { copiesBounds } from "./limits";

  let {
    part,
    edge,
    mode = $bindable("move"),
    op = $bindable(null),
  }: { part: RoofPart; edge: number | null; mode: TransformMode; op: TransformOp | null } = $props();

  // The pivot starts at the part's centre when the dialog opens (the core
  // gives it; a pivot typed meanwhile is kept).
  let dx = $state(0);
  let dy = $state(0);
  let angle = $state(90);
  let cx = $state(0);
  let cy = $state(0);
  let pivotTyped = false;
  untrack(() => {
    api.partCentre($state.snapshot(part) as RoofPart).then(
      (c) => {
        if (pivotTyped) return;
        cx = c.x;
        cy = c.y;
      },
      (e) => notifyError(e, "calc"),
    );
  });
  let axis = $state(90);
  let copies = $state(0);
  let dist = $state(0.5);

  const wallOk = $derived(edge !== null && !!part.edges[edge] && !isMarker(part.edges[edge]));
  const MODES: TransformMode[] = ["move", "rotate", "mirror", "wall"];

  /** The copies field refuses more than the mode takes, with the range (review UI-009). */
  const copyLimit = $derived(copiesBounds(mode));
  // Switching to Mirror with more copies typed: the field shows the one copy
  // the mirror makes instead of capping it silently.
  $effect(() => {
    if (copies > copyLimit.max) copies = copyLimit.max;
  });

  $effect(() => {
    const n = Math.max(copyLimit.min, Math.min(copyLimit.max, Math.round(copies || 0)));
    const fin = (...v: number[]) => v.every((x) => Number.isFinite(x));
    if (mode === "move") op = fin(dx, dy) && (dx || dy) ? { kind: "map", map: { kind: "move", dx, dy }, copies: n } : null;
    else if (mode === "rotate") op = fin(angle, cx, cy) && angle % 360 ? { kind: "map", map: { kind: "rotate", centre: { x: cx, y: cy }, deg: angle }, copies: n } : null;
    else if (mode === "mirror") op = fin(axis, cx, cy) ? { kind: "map", map: { kind: "mirror", centre: { x: cx, y: cy }, deg: axis }, copies: n } : null;
    else op = wallOk && fin(dist) && dist ? { kind: "wall", edge: edge!, dist } : null;
  });
</script>

<div class="form">
  <div class="seg" role="radiogroup" use:radiogroup aria-label={t("xf.title")}>
    {#each MODES as m}
      <button class:on={mode === m} role="radio" aria-checked={mode === m} disabled={m === "wall" && !wallOk} onclick={() => (mode = m)}>{t(`xf.${m}`)}</button>
    {/each}
  </div>
  <p class="hint">{t(`xf.${mode}Hint`)}</p>
  <div class="fields">
    {#if mode === "move"}
      <label><span>{t("xf.dx")}</span><div class="unit"><input type="text" use:numeric={{ value: dx, step: 0.1, live: true, onchange: (v) => (dx = v) }} /><em>m</em></div></label>
      <label><span>{t("xf.dy")}</span><div class="unit"><input type="text" use:numeric={{ value: dy, step: 0.1, live: true, onchange: (v) => (dy = v) }} /><em>m</em></div></label>
    {:else if mode === "rotate" || mode === "mirror"}
      {#if mode === "rotate"}
        <label><span>{t("xf.angle")}</span><div class="unit"><input type="text" use:numeric={{ value: angle, step: 1, live: true, onchange: (v) => (angle = v) }} /><em>°</em></div></label>
      {:else}
        <label><span>{t("xf.axis")}</span><div class="unit"><input type="text" use:numeric={{ value: axis, step: 1, live: true, onchange: (v) => (axis = v) }} /><em>°</em></div></label>
      {/if}
      <label><span>{t("xf.cx")}</span><div class="unit"><input type="text" use:numeric={{ value: cx, step: 0.1, live: true, onchange: (v) => ((pivotTyped = true), (cx = v)) }} /><em>m</em></div></label>
      <label><span>{t("xf.cy")}</span><div class="unit"><input type="text" use:numeric={{ value: cy, step: 0.1, live: true, onchange: (v) => ((pivotTyped = true), (cy = v)) }} /><em>m</em></div></label>
    {:else}
      <label><span>{t("xf.dist")}</span><div class="unit"><input type="text" use:numeric={{ value: dist, step: 0.05, live: true, onchange: (v) => (dist = v) }} /><em>m</em></div></label>
    {/if}
    {#if mode !== "wall"}
      <label title={t("xf.copiesHint")}>
        <span>{t("xf.copies")}</span>
        <input type="text" data-testid="xf-copies" aria-describedby="xf-copies-max" use:numeric={{ value: copies, step: 1, min: copyLimit.min, max: copyLimit.max, live: true, onchange: (v) => (copies = Math.round(v)) }} />
        <small class="max" id="xf-copies-max">{t("xf.copiesMax", { n: copyLimit.max })}</small>
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
  .max {
    display: block;
    margin-top: 2px;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .unit em {
    font-style: normal;
    color: var(--text-2);
    font-size: 12px;
  }
</style>
