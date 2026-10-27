<script lang="ts">
  // DACAD's calculation helpers for the selected wall: pitch from a simple
  // triangle, two facing walls meeting in a ridge, a point in space the plane
  // must reach, and merging parallel planes into one (DACAD.HLP: Neigungen /
  // Traufenhöhen / Vorsprünge berechnen). The result is a patch per wall.
  import { num } from "./format";
  import { t } from "./i18n.svelte";
  import { oppositeWall, spanBetween } from "./roundroof";
  import { inward, meeting, mergeOnto, parallels, rafter, roofPoints, sameWay, solve, triangle, type CalcTarget } from "./roofcalc";
  import type { CalcResult, EdgeDef, RoofPart } from "./types";

  let {
    part,
    edge,
    result,
    patches = $bindable(null),
  }: { part: RoofPart; edge: number; result: CalcResult | null; patches: Map<number, Partial<EdgeDef>> | null } = $props();

  type Tool = "triangle" | "parallel" | "point" | "merge";
  let tool = $state<Tool>("triangle");
  let target = $state<CalcTarget>("pitch");

  const es = $derived(part.edges);
  const e = $derived(es[edge]);
  const opp = $derived(oppositeWall(es, edge));
  const span = $derived(opp !== null ? spanBetween(es, edge, opp) : 0);

  // Einfaches Dreieck: two of three measures.
  let triWidth = $state<number | null>(null);
  let triRafter = $state<number | null>(null);
  let triRise = $state<number | null>(null);
  let triWithOv = $state(false);
  const tri = $derived(
    triangle(e, { width: triWidth ?? undefined, rafter: triRafter ?? undefined, rise: triRise ?? undefined }, triWithOv),
  );

  // 2 Parallele: ridge position and height, starting from the current ridge.
  const current = $derived(opp !== null ? meeting(e, es[opp], span) : null);
  let parW = $state<number | null>(null);
  let parH = $state<number | null>(null);
  const par = $derived.by(() => {
    if (opp === null || !current) return null;
    return parallels(e, es[opp], { span, widthA: parW ?? current.w, ridge: parH ?? current.h, target });
  });

  // Punkt im Raum: a computed roof point or one typed in.
  const points = $derived(roofPoints(result));
  let pick = $state(0);
  let px = $state<number | null>(null);
  let py = $state<number | null>(null);
  let pz = $state<number | null>(null);
  const point = $derived.by(() => {
    const q = points[pick]?.p;
    const x = px ?? q?.[0];
    const y = py ?? q?.[1];
    const z = pz ?? q?.[2];
    return x != null && y != null && z != null ? { x, y, z } : null;
  });
  const pt = $derived(point ? solve(e, inward(es, edge, point), point.z, target) : null);

  // Flächen zusammenfassen: parallel walls take this wall's plane.
  const mates = $derived(sameWay(es, edge).filter((k) => k !== edge));
  let chosen = $state<Set<number>>(new Set());
  $effect(() => {
    chosen = new Set(mates);
  });
  const merged = $derived(target === "pitch" ? null : mergeOnto(es, edge, [...chosen], target));

  $effect(() => {
    let next: Map<number, Partial<EdgeDef>> | null = null;
    if (tool === "triangle" && tri) {
      // With breaks the triangle is that of the last break (DACAD.HLP).
      const bs = e.breaks ?? [];
      const patch: Partial<EdgeDef> = bs.length
        ? { breaks: bs.map((b, i) => (i === bs.length - 1 ? { ...b, pitch_deg: tri.pitch_deg } : { ...b })) }
        : { pitch_deg: tri.pitch_deg };
      next = new Map([[edge, patch]]);
    }
    else if (tool === "parallel" && par && opp !== null) next = new Map([[edge, par[0]], [opp, par[1]]]);
    else if (tool === "point" && pt) next = new Map([[edge, pt]]);
    else if (tool === "merge" && merged && merged.size) next = merged;
    patches = next;
  });

  function n(v: string): number | null {
    const x = parseFloat(v.replace(",", "."));
    return Number.isFinite(x) ? x : null;
  }
  const fmt = (k: string, v: unknown) =>
    k === "breaks"
      ? (v as { height: number; pitch_deg: number }[]).map((b) => `${num(b.height)} m / ${num(b.pitch_deg)}°`).join(", ")
      : k === "pitch_deg"
        ? `${num(v as number)}°`
        : `${num(v as number)} m`;
  const label = (k: string) =>
    k === "pitch_deg" ? t("edge.pitch") : k === "eave_height" ? t("edge.eaveHeight") : k === "breaks" ? t("edge.breaks") : t("edge.overhang");
</script>

<div class="form">
  <div class="tabs" role="tablist">
    {#each ["triangle", "parallel", "point", "merge"] as const as k}
      <button role="tab" class:on={tool === k} aria-selected={tool === k} disabled={k === "parallel" && opp === null}
        onclick={() => { tool = k; if (k === "triangle") target = "pitch"; else if (k === "merge" && target === "pitch") target = "eave"; }}>{t(`calc.${k}`)}</button>
    {/each}
  </div>
  <p class="tip">{t(`calc.${tool}Hint`)}</p>

  {#if tool !== "triangle"}
    <label class="target">
      <span>{t("calc.target")}</span>
      <select bind:value={target}>
        {#if tool !== "merge"}<option value="pitch">{t("edge.pitch")}</option>{/if}
        <option value="eave">{t("edge.eaveHeight")}</option>
        <option value="overhang">{t("edge.overhang")}</option>
      </select>
    </label>
  {/if}

  {#if tool === "triangle"}
    <div class="grid">
      <label><span>{t("calc.width")}</span><div class="unit"><input type="number" step="0.05" value={triWidth ?? ""} oninput={(ev) => (triWidth = n(ev.currentTarget.value))} /><em>m</em></div></label>
      <label><span>{t("calc.rafter")}</span><div class="unit"><input type="number" step="0.05" value={triRafter ?? ""} oninput={(ev) => (triRafter = n(ev.currentTarget.value))} /><em>m</em></div></label>
      <label><span>{t("calc.rise")}</span><div class="unit"><input type="number" step="0.05" value={triRise ?? ""} oninput={(ev) => (triRise = n(ev.currentTarget.value))} /><em>m</em></div></label>
      <label class="check"><input type="checkbox" bind:checked={triWithOv} />{t("calc.withOverhang")}</label>
    </div>
  {:else if tool === "parallel" && current}
    <div class="grid">
      <label><span>{t("calc.widthA")}</span><div class="unit"><input type="number" step="0.05" value={+(parW ?? current.w).toFixed(3)} onchange={(ev) => (parW = n(ev.currentTarget.value))} /><em>m</em></div></label>
      <label><span>{t("calc.widthB")}</span><div class="unit"><input type="number" step="0.05" value={+(span - (parW ?? current.w)).toFixed(3)} onchange={(ev) => { const v = n(ev.currentTarget.value); parW = v === null ? null : span - v; }} /><em>m</em></div></label>
      <label><span>{t("calc.ridge")}</span><div class="unit"><input type="number" step="0.05" value={+(parH ?? current.h).toFixed(3)} onchange={(ev) => (parH = n(ev.currentTarget.value))} /><em>m</em></div></label>
    </div>
    <p class="tip">{t("round.span", { s: num(span) })}</p>
  {:else if tool === "point"}
    <div class="grid">
      <label class="wide">
        <span>{t("calc.roofPoint")}</span>
        <select value={pick} onchange={(ev) => { pick = +ev.currentTarget.value; px = py = pz = null; }}>
          {#each points as q, i}
            <option value={i}>{t(`calc.kind.${["ridge", "hip", "valley"].includes(q.kind) ? q.kind : "other"}`)} · X {num(q.p[0])} · Y {num(q.p[1])} · Z {num(q.p[2])}</option>
          {/each}
        </select>
      </label>
      <label><span>X</span><div class="unit"><input type="number" step="0.05" value={point ? +point.x.toFixed(3) : ""} onchange={(ev) => (px = n(ev.currentTarget.value))} /><em>m</em></div></label>
      <label><span>Y</span><div class="unit"><input type="number" step="0.05" value={point ? +point.y.toFixed(3) : ""} onchange={(ev) => (py = n(ev.currentTarget.value))} /><em>m</em></div></label>
      <label><span>Z</span><div class="unit"><input type="number" step="0.05" value={point ? +point.z.toFixed(3) : ""} onchange={(ev) => (pz = n(ev.currentTarget.value))} /><em>m</em></div></label>
    </div>
  {:else if tool === "merge"}
    {#if mates.length}
      <div class="walls">
        {#each mates as k}
          <label class="check">
            <input type="checkbox" checked={chosen.has(k)}
              onchange={(ev) => { const s = new Set(chosen); if (ev.currentTarget.checked) s.add(k); else s.delete(k); chosen = s; }} />
            {t("calc.wall", { n: k + 1 })}
          </label>
        {/each}
      </div>
    {:else}
      <p class="err">{t("calc.noMates")}</p>
    {/if}
  {/if}

  <div class="result">
    {#if patches && patches.size}
      {#each [...patches] as [k, patch]}
        <div class="row">
          <strong>{t("calc.wall", { n: k + 1 })}</strong>
          {#each Object.entries(patch) as [field, v]}
            <span>{label(field)}: {fmt(field, es[k][field as keyof EdgeDef])} → <b>{fmt(field, v)}</b></span>
          {/each}
          {#if tool === "parallel" && current && !es[k].breaks?.length}
            <span class="muted">{t("calc.rafter")}: {num(rafter({ ...es[k], ...patch } as EdgeDef, k === edge ? (parW ?? current.w) : span - (parW ?? current.w)))} m</span>
          {/if}
        </div>
      {/each}
    {:else if (tool !== "triangle" && (tool !== "merge" || mates.length)) || triWidth !== null || triRafter !== null || triRise !== null}
      <p class="err">{t("calc.noResult")}</p>
    {/if}
  </div>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 3px;
    background: var(--plan-fill);
    border-radius: 8px;
  }
  .tabs button {
    flex: 1;
    padding: 7px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .tabs button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
  }
  .tabs button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px 12px;
    align-items: end;
  }
  .wide {
    grid-column: 1 / -1;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 100%;
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
  select {
    width: 100%;
    height: 34px;
  }
  .target {
    max-width: 220px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .check input {
    width: auto;
    flex-shrink: 0;
  }
  .walls {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
  }
  .result {
    padding: 10px 12px;
    background: var(--plan-fill);
    border-radius: 8px;
    font-size: 13px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    padding: 3px 0;
  }
  .muted {
    color: var(--text-2);
  }
  .tip,
  .err {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .err {
    color: var(--danger);
  }
</style>
