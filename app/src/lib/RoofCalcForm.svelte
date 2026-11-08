<script lang="ts">
  // DACAD's calculation helpers for the selected wall: pitch from a simple
  // triangle, two facing walls meeting in a ridge, a point in space the plane
  // must reach, merging parallel planes into one, pitches from a hip or
  // valley (DACAD.HLP: Neigungen / Traufenhöhen / Vorsprünge / Lage
  // berechnen) and the build-up correction (Um Aufbaudicke korrigieren).
  // "position" moves walls instead. The result is a patch per wall. The
  // rules were measured in DACAD itself (roofcalc.test.ts).
  import { num } from "./format";
  import { t } from "./i18n.svelte";
  import { numeric } from "./numeric.svelte";
  import { oppositeWall, spanBetween } from "./roundroof";
  import { buildUp, combine, eaveBelowGable, hipPitches, hipPitchesFromEaves, inward, meeting, mergeByMoving, mergeOnto, moveWall, parallelEdit, parallelPatches, parallelRafter, parallelStart, roofPoints, sameWay, shiftFor, solve, triangle, trianglePatch, type BuildUpEave, type CalcTarget, type ParallelField } from "./roofcalc";
  import type { CalcResult, EdgeDef, RoofPart } from "./types";

  let {
    part,
    edge,
    result,
    patches = $bindable(null),
  }: { part: RoofPart; edge: number; result: CalcResult | null; patches: Map<number, Partial<EdgeDef>> | null } = $props();

  type Tool = "triangle" | "parallel" | "point" | "merge" | "hip" | "buildup";
  type Target = CalcTarget | "position";
  let tool = $state<Tool>("triangle");
  let target = $state<Target>("pitch");

  const es = $derived(part.edges);
  const e = $derived(es[edge]);
  const opp = $derived(oppositeWall(es, edge));
  const span = $derived(opp !== null ? spanBetween(es, edge, opp) : 0);

  // Einfaches Dreieck: two of width, rafter and ridge height (absolute).
  let triWidth = $state<number | null>(null);
  let triRafter = $state<number | null>(null);
  let triRidge = $state<number | null>(null);
  // DACAD's dialog opens with "Breite inkl. Traufenvorsprung" off.
  let triWithOv = $state(false);
  const tri = $derived(
    triangle(e, { width: triWidth ?? undefined, rafter: triRafter ?? undefined, ridge: triRidge ?? undefined }, triWithOv),
  );

  // 2 Parallele: the values typed in, applied in order as DACAD's dialog does.
  const current = $derived(opp !== null ? meeting(e, es[opp], span) : null);
  const parBreaks = $derived(opp !== null && !!(e.breaks?.length || es[opp].breaks?.length));
  // DACAD's dialog opens with "Grundrissbreite inkl. Traufenvorsprung" on.
  let parWithOv = $state(true);
  let parEdits = $state<[ParallelField, number][]>([]);
  $effect(() => {
    void edge;
    void target;
    void parWithOv;
    parEdits = [];
  });
  const parState = $derived.by(() => {
    if (opp === null || !current || parBreaks) return null;
    let st = parallelStart(e, es[opp], span, current);
    for (const [f, v] of parEdits) {
      const next = parallelEdit(st, f, v, target, parWithOv);
      if (!next) return null;
      st = next;
    }
    return st;
  });
  const parRefused = $derived.by(() => {
    if (opp === null || !parState || target !== "eave") return false;
    const [pa, pb] = parallelPatches(parState, "eave");
    return eaveBelowGable(es, edge, pa.eave_height!) || eaveBelowGable(es, opp, pb.eave_height!);
  });
  const par = $derived.by(() => {
    if (opp === null || !parState || !parEdits.length || parRefused) return null;
    if (target === "position") return combine(moveWall(es, edge, parState.shiftA), moveWall(es, opp, parState.shiftB));
    const [pa, pb] = parallelPatches(parState, target);
    return new Map([[edge, pa], [opp, pb]]);
  });
  const parSet = (f: ParallelField, v: number) => {
    parEdits = [...parEdits, [f, v]];
  };

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
  const pt = $derived.by(() => {
    if (!point) return null;
    const d = inward(es, edge, point);
    if (target === "position") {
      const sh = shiftFor(e, d, point.z);
      return sh === null ? null : moveWall(es, edge, sh);
    }
    const p = solve(e, d, point.z, target);
    return p && new Map([[edge, p]]);
  });

  // Flächen zusammenfassen: parallel walls take this wall's plane.
  const mates = $derived(sameWay(es, edge).filter((k) => k !== edge));
  let chosen = $state<Set<number>>(new Set());
  $effect(() => {
    chosen = new Set(mates);
  });
  const merged = $derived.by(() => {
    if (target === "pitch") return null;
    if (target !== "position") return mergeOnto(es, edge, [...chosen], target);
    // Each wall moves until its eave (at its overhang) lies on this wall's plane.
    return mergeByMoving(es, edge, [...chosen]);
  });

  // Grat/Kehle: hip or valley length and both rafter lengths.
  let side = $state<"next" | "prev">("next");
  const neighbour = $derived(side === "next" ? (edge + 1) % es.length : (edge - 1 + es.length) % es.length);
  let hipBy = $state<"rafters" | "eaves">("rafters");
  let hipLen = $state<number | null>(null);
  let raf1 = $state<number | null>(null);
  let raf2 = $state<number | null>(null);
  const hip = $derived(
    hipLen !== null && raf1 !== null && raf2 !== null
      ? (hipBy === "rafters" ? hipPitches : hipPitchesFromEaves)(es, edge, neighbour, hipLen, raf1, raf2)
      : null,
  );

  // Um Aufbaudicke korrigieren.
  let thick = $state<number | null>(null);
  let eaveForm = $state<BuildUpEave>("normal");
  const built = $derived(thick !== null ? buildUp(e, thick, eaveForm) : null);

  $effect(() => {
    let next: Map<number, Partial<EdgeDef>> | null = null;
    if (tool === "triangle" && tri) next = new Map([[edge, trianglePatch(e, tri.pitch_deg)]]);
    else if (tool === "parallel" && par) next = par;
    else if (tool === "point" && pt) next = pt;
    else if (tool === "merge" && merged && merged.size) next = merged;
    else if (tool === "hip" && hip) next = hip;
    else if (tool === "buildup" && built) next = new Map([[edge, built]]);
    // Leave out fields that would not change.
    if (next) {
      const same = (x: unknown, y: unknown) => JSON.stringify(x) === JSON.stringify(y);
      next = new Map(
        [...next]
          .map(([k, patch]) => [k, Object.fromEntries(Object.entries(patch).filter(([f, v]) => !same(es[k][f as keyof EdgeDef], v)))] as const)
          .filter(([, patch]) => Object.keys(patch).length > 0),
      );
    }
    unchanged = next !== null && next.size === 0;
    patches = next && next.size ? next : null;
  });

  /** The inputs give the current state: nothing to apply. */
  let unchanged = $state(false);
  /** Whether the inputs are complete enough to say there is no result. */
  const touched = $derived(
    (tool === "point" || (tool === "merge" && mates.length)) ||
      (tool === "parallel" && parEdits.length > 0) ||
      (tool === "buildup" && thick !== null) ||
      (tool === "triangle" && (triWidth !== null || triRafter !== null || triRidge !== null)) ||
      (tool === "hip" && hipLen !== null && raf1 !== null && raf2 !== null),
  );

  const fmt = (k: string, v: unknown) =>
    k === "start"
      ? `X ${num((v as { x: number }).x)} · Y ${num((v as { y: number }).y)}`
      : k === "breaks"
      ? (v as { height: number; pitch_deg: number }[]).map((b) => `${num(b.height)} m / ${num(b.pitch_deg)}°`).join(", ")
      : k === "pitch_deg"
        ? `${num(v as number)}°`
        : `${num(v as number)} m`;
  const label = (k: string) =>
    k === "pitch_deg"
      ? t("edge.pitch")
      : k === "eave_height"
        ? t("edge.eaveHeight")
        : k === "breaks"
          ? t("edge.breaks")
          : k === "start"
            ? t("calc.start")
            : t("edge.overhang");
</script>

<div class="form">
  <div class="tabs" role="tablist">
    {#each ["triangle", "parallel", "point", "merge", "hip", "buildup"] as const as k}
      <button role="tab" class:on={tool === k} aria-selected={tool === k} disabled={k === "parallel" && opp === null}
        onclick={() => { tool = k; if (k === "triangle" || k === "hip" || k === "buildup") target = "pitch"; else if (k === "merge" && target === "pitch") target = "eave"; }}>{t(`calc.${k}`)}</button>
    {/each}
  </div>
  <p class="tip">{t(`calc.${tool}Hint`)}</p>

  {#if tool !== "triangle" && tool !== "hip" && tool !== "buildup"}
    <label class="target">
      <span>{t("calc.target")}</span>
      <select bind:value={target}>
        {#if tool !== "merge"}<option value="pitch">{t("edge.pitch")}</option>{/if}
        <option value="eave">{t("edge.eaveHeight")}</option>
        <option value="overhang">{t("edge.overhang")}</option>
        <option value="position">{t("calc.position")}</option>
      </select>
    </label>
  {/if}

  {#if tool === "triangle"}
    <div class="grid">
      <label><span>{t("calc.width")}</span><div class="unit"><input type="text" use:numeric={{ value: triWidth, step: 0.05, live: true, onchange: (v) => (triWidth = v), onclear: () => (triWidth = null) }} /><em>m</em></div></label>
      <label><span>{t("calc.rafter")}</span><div class="unit"><input type="text" use:numeric={{ value: triRafter, step: 0.05, live: true, onchange: (v) => (triRafter = v), onclear: () => (triRafter = null) }} /><em>m</em></div></label>
      <label><span>{t("calc.ridge")}</span><div class="unit"><input type="text" use:numeric={{ value: triRidge, step: 0.05, live: true, onchange: (v) => (triRidge = v), onclear: () => (triRidge = null) }} /><em>m</em></div></label>
      <label class="check"><input type="checkbox" bind:checked={triWithOv} />{t("calc.withOverhang")}</label>
    </div>
  {:else if tool === "parallel" && current}
    {#if parBreaks}
      <p class="err">{t("calc.noBreaks")}</p>
    {:else}
    {@const st = parState}
    {@const ovA = parWithOv && st ? st.a.overhang : 0}
    {@const ovB = parWithOv && st ? st.b.overhang : 0}
    <div class="grid">
      <label><span>{t("calc.widthA")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +(st.wA + ovA).toFixed(3) : null, step: 0.05, onchange: (v) => parSet("widthA", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.widthB")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +(st.wB + ovB).toFixed(3) : null, step: 0.05, onchange: (v) => parSet("widthB", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.ridge")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +st.H.toFixed(3) : null, step: 0.05, onchange: (v) => parSet("ridge", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.rafterA")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +parallelRafter(st, "a").toFixed(3) : null, step: 0.05, onchange: (v) => parSet("rafterA", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.rafterB")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +parallelRafter(st, "b").toFixed(3) : null, step: 0.05, onchange: (v) => parSet("rafterB", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.top")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +st.top.toFixed(3) : null, step: 0.05, min: 0, onchange: (v) => parSet("top", v) }} /><em>m</em></div></label>
      <label class="check wide"><input type="checkbox" bind:checked={parWithOv} />{t("calc.withOverhang")}</label>
    </div>
    <p class="tip">{t("round.span", { s: num(span) })}</p>
    {#if parRefused}<p class="err">{t("calc.eaveBelowGable")}</p>{/if}
    {/if}
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
      <label><span>X</span><div class="unit"><input type="text" use:numeric={{ value: point ? +point.x.toFixed(3) : null, step: 0.05, onchange: (v) => (px = v) }} /><em>m</em></div></label>
      <label><span>Y</span><div class="unit"><input type="text" use:numeric={{ value: point ? +point.y.toFixed(3) : null, step: 0.05, onchange: (v) => (py = v) }} /><em>m</em></div></label>
      <label><span>Z</span><div class="unit"><input type="text" use:numeric={{ value: point ? +point.z.toFixed(3) : null, step: 0.05, onchange: (v) => (pz = v) }} /><em>m</em></div></label>
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
  {:else if tool === "hip"}
    <div class="grid">
      <label class="wide">
        <span>{t("calc.neighbour")}</span>
        <select bind:value={side}>
          <option value="next">{t("calc.wall", { n: ((edge + 1) % es.length) + 1 })}</option>
          <option value="prev">{t("calc.wall", { n: ((edge - 1 + es.length) % es.length) + 1 })}</option>
        </select>
      </label>
      <label class="wide">
        <span>{t("calc.hipBy")}</span>
        <select bind:value={hipBy}>
          <option value="rafters">{t("calc.hipByRafters")}</option>
          <option value="eaves">{t("calc.hipByEaves")}</option>
        </select>
      </label>
      <label><span>{t("calc.hipLength")}</span><div class="unit"><input type="text" use:numeric={{ value: hipLen, step: 0.05, live: true, onchange: (v) => (hipLen = v), onclear: () => (hipLen = null) }} /><em>m</em></div></label>
      <label><span>{t(hipBy === "rafters" ? "calc.rafterThis" : "calc.eaveThis")}</span><div class="unit"><input type="text" use:numeric={{ value: raf1, step: 0.05, live: true, onchange: (v) => (raf1 = v), onclear: () => (raf1 = null) }} /><em>m</em></div></label>
      <label><span>{t(hipBy === "rafters" ? "calc.rafterOther" : "calc.eaveOther")}</span><div class="unit"><input type="text" use:numeric={{ value: raf2, step: 0.05, live: true, onchange: (v) => (raf2 = v), onclear: () => (raf2 = null) }} /><em>m</em></div></label>
    </div>
  {:else if tool === "buildup"}
    <div class="grid">
      <label><span>{t("calc.thickness")}</span><div class="unit"><input type="text" use:numeric={{ value: thick, step: 0.01, live: true, onchange: (v) => (thick = v), onclear: () => (thick = null) }} /><em>m</em></div></label>
      <label class="wide">
        <span>{t("calc.eaveForm")}</span>
        <select bind:value={eaveForm}>
          {#each ["normal", "horizontal", "vertical"] as const as f}<option value={f}>{t(`calc.eave.${f}`)}</option>{/each}
        </select>
      </label>
    </div>
  {/if}

  {#if (patches && patches.size) || touched}
  <div class="result">
    {#if patches && patches.size}
      {#each [...patches] as [k, patch]}
        <div class="row">
          <strong>{t("calc.wall", { n: k + 1 })}</strong>
          {#each Object.entries(patch) as [field, v]}
            <span>{label(field)}: {fmt(field, es[k][field as keyof EdgeDef])} → <b>{fmt(field, v)}</b></span>
          {/each}
        </div>
      {/each}
    {:else if unchanged}
      <p class="tip">{t("calc.noChange")}</p>
    {:else}
      <p class="err">{t("calc.noResult")}</p>
    {/if}
  </div>
  {/if}
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
