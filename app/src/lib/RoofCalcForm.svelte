<script lang="ts">
  // DACAD's calculation helpers for the selected wall: pitch from a simple
  // triangle, two facing walls meeting in a ridge, a point in space the plane
  // must reach, merging parallel planes into one, pitches from a hip or
  // valley (DACAD.HLP: Neigungen / Traufenhöhen / Vorsprünge / Lage
  // berechnen) and the build-up correction (Um Aufbaudicke korrigieren).
  // "position" moves walls instead. The result is a patch per wall. The
  // rules were measured in DACAD itself (tectora-core `edit/calc.rs` tests).
  import { onDestroy } from "svelte";
  import { api } from "./api";
  import { num } from "./format";
  import { t } from "./i18n.svelte";
  import { latest } from "./latest";
  import { notifyError } from "./notify.svelte";
  import { tabIds, tablist } from "./tabs";
  import { numeric } from "./numeric.svelte";
  import { emptyProject } from "./types";
  import type { BuildUpEave, CalcTarget, EdgeDef, HelperContext, HelperPreview, HelperRequest, ParallelField, Project, RoofPart } from "./types";

  // Every figure comes from the core (`roof_helper_context`, `roof_helper_preview`);
  // this form only holds what is typed, asks again after a pause and shows the
  // answer. OK applies `request` through the core (`roof_helper_apply`).
  let {
    project,
    partIndex,
    edge,
    request = $bindable(null),
  }: { project: Project; partIndex: number; edge: number; request: HelperRequest | null } = $props();
  const ids = tabIds("calc");

  type Tool = "triangle" | "parallel" | "point" | "merge" | "hip" | "buildup";
  type Target = CalcTarget | "position";
  let tool = $state<Tool>("triangle");
  let target = $state<Target>("pitch");

  const es = $derived(project.parts[partIndex].edges);
  const e = $derived(es[edge]);

  /** A project of this part alone, which is all a helper reads (the point list needs the whole project). */
  const only = (part: RoofPart): Project => {
    const p = emptyProject("helper");
    p.parts = [part];
    return p;
  };
  const failed = (err: unknown) => notifyError(err, "calc");

  // What the wall's surroundings allow: the facing wall and span, parallel walls, roof points.
  let ctx = $state<HelperContext | null>(null);
  const askCtx = latest(
    (proj: Project, part: number, withPoints: boolean) => api.roofHelperContext(proj, part, edge, withPoints),
    (c) => (ctx = c),
    failed,
  );
  $effect(() => {
    const withPoints = tool === "point";
    // The point tool also lists the roof's corners: the whole project.
    if (withPoints) askCtx.ask($state.snapshot(project) as Project, partIndex, true);
    else askCtx.ask(only($state.snapshot(project.parts[partIndex]) as RoofPart), 0, false);
  });
  const opp = $derived(ctx?.opposite ?? null);
  const span = $derived(ctx?.span ?? 0);
  const current = $derived(ctx?.current ?? null);
  const parBreaks = $derived(!!ctx?.breaks);
  const mates = $derived(ctx?.mates ?? []);

  // Einfaches Dreieck: two of width, rafter and ridge height (absolute).
  let triWidth = $state<number | null>(null);
  let triRafter = $state<number | null>(null);
  let triRidge = $state<number | null>(null);
  // The dialog opens with "width incl. overhang" off.
  let triWithOv = $state(false);

  // 2 Parallele: the values typed in, applied in order.
  // The dialog opens with "plan width incl. overhang" on.
  let parWithOv = $state(true);
  let parEdits = $state<[ParallelField, number][]>([]);
  $effect(() => {
    void edge;
    void target;
    void parWithOv;
    parEdits = [];
  });
  const parSet = (f: ParallelField, v: number) => {
    parEdits = [...parEdits, [f, v]];
  };

  // Punkt im Raum: a computed roof point or one typed in.
  const points = $derived(ctx?.roof_points ?? []);
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

  // Flächen zusammenfassen: parallel walls take this wall's plane.
  let chosen = $state<Set<number>>(new Set());
  $effect(() => {
    chosen = new Set(mates);
  });

  // Grat/Kehle: hip or valley length and both rafter lengths.
  let side = $state<"next" | "prev">("next");
  let hipBy = $state<"rafters" | "eaves">("rafters");
  let hipLen = $state<number | null>(null);
  let raf1 = $state<number | null>(null);
  let raf2 = $state<number | null>(null);

  // Um Aufbaudicke korrigieren.
  let thick = $state<number | null>(null);
  let eaveForm = $state<BuildUpEave>("normal");

  /** The request the typed values make, or null while they are not complete. */
  const req = $derived.by((): HelperRequest | null => {
    if (tool === "triangle") {
      if (triWidth === null && triRafter === null && triRidge === null) return null;
      return { tool: "triangle", width: triWidth, rafter: triRafter, ridge: triRidge, width_includes_overhang: triWithOv };
    }
    if (tool === "parallel") {
      if (opp === null || !current || parBreaks) return null;
      return { tool: "parallel", target, steps: parEdits.map(([field, value]) => ({ field, value })), width_includes_overhang: parWithOv };
    }
    if (tool === "point") return point ? { tool: "point", target, point: [point.x, point.y, point.z] } : null;
    if (tool === "merge") return target === "pitch" ? null : { tool: "merge", target, walls: [...chosen] };
    if (tool === "hip") {
      if (hipLen === null || raf1 === null || raf2 === null) return null;
      return { tool: "hip", neighbour: side, by: hipBy, hip: hipLen, first: raf1, second: raf2 };
    }
    return thick === null ? null : { tool: "build_up", thickness: thick, eave: eaveForm };
  });

  // The core's answer to a request; null: no solution for the values.
  let answered = $state<{ req: HelperRequest; res: HelperPreview | null } | null>(null);
  const askPreview = latest(
    (r: HelperRequest, part: RoofPart) => api.roofHelperPreview(only(part), 0, edge, r).then((res) => ({ req: r, res })),
    (a) => (answered = a),
    (err) => {
      answered = null;
      failed(err);
    },
  );
  onDestroy(() => {
    askCtx.stop();
    askPreview.stop();
  });
  $effect(() => {
    if (!req) {
      askPreview.stop();
      answered = null;
      return;
    }
    askPreview.ask(req, $state.snapshot(project.parts[partIndex]) as RoofPart);
  });
  /** The answer, once it is for the values on screen. */
  const settled = $derived(answered && req && JSON.stringify(answered.req) === JSON.stringify(req) ? answered : null);
  const res = $derived(answered?.res ?? null);
  /** Nothing is applied before a value was typed (the parallel dialog starts from the current state). */
  const typed = $derived(!(tool === "parallel" && !parEdits.length));
  const patches = $derived(res && typed && !res.refused ? res.patches : null);
  const unchanged = $derived(!!res && typed && res.unchanged);
  const parRefused = $derived(!!res?.refused);
  $effect(() => {
    request = settled?.res && typed && !settled.res.refused && settled.res.patches.size ? settled.req : null;
  });
  /** The values on screen have no answer yet. */
  const pending = $derived(!!req && !settled);
  /** Whether the inputs are complete enough to say there is no result. */
  const touched = $derived(
    (tool === "point") ||
      (tool === "merge" && mates.length > 0) ||
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
  <div class="tabs" role="tablist" use:tablist>
    {#each ["triangle", "parallel", "point", "merge", "hip", "buildup"] as const as k}
      <button role="tab" id={ids.tab(k)} aria-controls={ids.panel} class:on={tool === k} aria-selected={tool === k} disabled={k === "parallel" && opp === null}
        onclick={() => { tool = k; if (k === "triangle" || k === "hip" || k === "buildup") target = "pitch"; else if (k === "merge" && target === "pitch") target = "eave"; }}>{t(`calc.${k}`)}</button>
    {/each}
  </div>
  <div class="panel" role="tabpanel" id={ids.panel} aria-labelledby={ids.tab(tool)}>
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
    {@const st = res?.state ?? null}
    {@const ovA = parWithOv && st ? st.a.overhang : 0}
    {@const ovB = parWithOv && st ? st.b.overhang : 0}
    <div class="grid">
      <label><span>{t("calc.widthA")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +(st.w_a + ovA).toFixed(3) : null, step: 0.05, onchange: (v) => parSet("width_a", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.widthB")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +(st.w_b + ovB).toFixed(3) : null, step: 0.05, onchange: (v) => parSet("width_b", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.ridge")}</span><div class="unit"><input type="text" use:numeric={{ value: st ? +st.h.toFixed(3) : null, step: 0.05, onchange: (v) => parSet("ridge", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.rafterA")}</span><div class="unit"><input type="text" use:numeric={{ value: st && res?.rafters ? +res.rafters[0].toFixed(3) : null, step: 0.05, onchange: (v) => parSet("rafter_a", v) }} /><em>m</em></div></label>
      <label><span>{t("calc.rafterB")}</span><div class="unit"><input type="text" use:numeric={{ value: st && res?.rafters ? +res.rafters[1].toFixed(3) : null, step: 0.05, onchange: (v) => parSet("rafter_b", v) }} /><em>m</em></div></label>
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

  {#if (patches && patches.size) || (touched && !pending)}
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
</div>

<style>
  .form,
  .panel {
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
    box-shadow: var(--shadow);
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
