<script lang="ts">
  // How the roof's numbers were calculated, step by step, for whoever checks
  // an offer: the walls as entered, every face as plan area / cos(pitch)
  // less openings, every line as the 3D distance of its ends, and how they
  // add up per material and per line code. All numbers come from the core
  // (`calc_breakdown`); nothing is calculated here.
  import { api } from "./api";
  import { toAppError } from "./errors";
  import { num } from "./format";
  import { numberLocale, t } from "./i18n.svelte";
  import { materialName } from "./materials";
  import type { CalcBreakdown, CalcResult, PlanMark, Project } from "./types";

  let {
    project,
    result,
    data = $bindable(null),
    onmark,
  }: {
    project: Project;
    result: CalcResult | null;
    data?: CalcBreakdown | null;
    /** Highlights a face or line in the plan (null clears it). */
    onmark?: (mark: PlanMark) => void;
  } = $props();

  let error = $state<string | null>(null);
  let seq = 0;

  // Recalculate whenever the roof result changes (every edit recalculates it).
  $effect(() => {
    void result;
    const snapshot = $state.snapshot(project) as Project;
    const mine = ++seq;
    api
      .calcBreakdown(snapshot)
      .then((b) => {
        if (mine !== seq) return;
        data = b;
        error = null;
      })
      .catch((e) => {
        if (mine !== seq) return;
        error = toAppError(e).code;
      });
  });

  const fixed = (v: number, d: number) =>
    new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: d, maximumFractionDigits: d }).format(v);
  const a2 = (v: number) => `${num(v)} m²`;
  const len = (v: number) => `${num(v)} m`;
  const deg = (v: number) => `${fixed(v, Math.abs(v - Math.round(v)) < 1e-6 ? 0 : 1)}°`;

  const many = $derived((data?.parts.length ?? 0) > 1);
  const multiMaterial = $derived((data?.materials.length ?? 0) > 1);

  /** Face reference in sums: "3" for one part, "2.3" for several. */
  const faceRef = (part: number | null, face: number | null) =>
    part === null ? t("bd.manualRef", { n: face ?? 0 }) : face === null ? t("bd.cladRef", { n: part + 1 }) : many ? `${part + 1}.${face}` : `${face}`;

  const lineLabel = (code: string) => {
    const key: Record<string, string> = {
      T: "res.eave",
      F: "res.ridge",
      G: "res.hipShort",
      K: "res.valleyShort",
      OL: "bd.code.OL",
      OR: "bd.code.OR",
      OS: "bd.code.OS",
      P: "res.pult",
      c: "res.knick",
      S: "res.gorge",
    };
    return t(key[code] ?? "res.abutment");
  };

  const openingLabel = (k: string) =>
    t(k === "window" ? "opening.window" : k === "chimney" ? "opening.chimney" : k === "hole" ? "opening.hole" : "bd.dormer");

  const plan2 = (p: [number, number, number][]) => p.map((v) => [v[0], v[1]] as [number, number]);

  /** The sum of the totals shown in the results, to confirm they agree. */
  const agrees = $derived(data && result ? Math.abs(data.roof_area - result.totals.roof_area) < 0.005 : true);
</script>

<div class="bd" role="region" aria-label={t("bd.title")}>
  <p class="intro">{t("bd.intro")}</p>

  {#if error}
    <div class="err" role="status"><i></i><span>{t("bd.error")}</span><code>{error}</code></div>
  {:else if !data}
    <p class="muted">…</p>
  {:else if !data.parts.length && !data.manual_faces.length}
    <p class="muted">{t("bd.empty")}</p>
  {:else}
    {#each data.parts as part, pi}
      <section>
        <h4 class="part">
          <span>{many ? `${pi + 1} · ${part.name}` : part.name}</span>
          <b>{a2(part.roof_area)}</b>
        </h4>

        <details>
          <summary>{t("bd.walls", { n: part.walls.length })}</summary>
          <table class="walls">
            <thead>
              <tr>
                <th>#</th><th>{t("bd.col.length")}</th><th>{t("edge.pitch")}</th><th>{t("bd.col.eave")}</th><th>{t("edge.overhang")}</th>
                {#if multiMaterial}<th>{t("bd.col.mat")}</th>{/if}
              </tr>
            </thead>
            <tbody>
              {#each part.walls as w}
                <tr>
                  <td>{w.no}</td>
                  <td>{num(w.length)}</td>
                  <td>
                    {w.gable ? t("edge.gable") : deg(w.pitch_deg)}{#if w.half_hip}<span class="tag">{t("bd.halfHip")}</span>{/if}{#if w.breaks}<span class="tag">{t("bd.breaks", { n: w.breaks })}</span>{/if}{#if w.clad}<span class="tag">{t("bd.clad")}</span>{/if}
                  </td>
                  <td>{num(w.eave_height)}</td>
                  <td>{num(w.overhang)}</td>
                  {#if multiMaterial}<td>{w.material}</td>{/if}
                </tr>
              {/each}
            </tbody>
          </table>
        </details>

        {#if !part.calculated}
          <p class="muted">{t("bd.notCalculated")}</p>
        {/if}

        {#each part.faces as f}
          <div
            class="face"
            role="listitem"
            tabindex="-1"
            onpointerenter={() => onmark?.({ polygon: plan2(f.polygon) })}
            onpointerleave={() => onmark?.(null)}
          >
            <div class="face-head">
              <span class="no">{faceRef(pi, f.no)}</span>
              <span>{f.wall !== null ? t("bd.wall", { n: f.wall }) : t("bd.dormer")}</span>
              {#if multiMaterial}<span class="mat">{materialName(project, f.material)}</span>{/if}
              <b>{a2(f.area)}</b>
            </div>
            <div class="calc">
              <span>{fixed(f.plan_area, 3)} m² ÷ cos {deg(f.pitch_deg)}</span>
              <span>= {fixed(f.plan_area, 3)} × {fixed(f.factor, 4)} = {a2(f.gross)}</span>
              {#if f.engine_gross !== undefined}
                <span class="note">{t("bd.engine", { v: a2(f.engine_gross), d: a2(f.engine_gross - f.gross) })}</span>
              {/if}
              {#if f.deducted > 0.00005}
                <span>− {a2(f.deducted)} <em>{t("bd.deducted")}</em> = {a2(f.area)}</span>
              {/if}
            </div>
            <details class="corners">
              <summary>{t("bd.corners", { n: f.polygon.length })}</summary>
              <div class="calc">
                {#each f.polygon as v}<span>{num(v[0])}; {num(v[1])}; {num(v[2])}</span>{/each}
              </div>
            </details>
          </div>
        {/each}

        {#each part.clad ?? [] as [mat, a]}
          <div class="face">
            <div class="face-head">
              <span class="no">{faceRef(pi, null)}</span>
              <span>{t("bd.cladGables")}</span>
              {#if multiMaterial}<span class="mat">{materialName(project, mat)}</span>{/if}
              <b>{a2(a)}</b>
            </div>
          </div>
        {/each}

        {#if part.openings?.length}
          <div class="sub">{t("bd.openings")}</div>
          <table>
            <tbody>
              {#each part.openings as o}
                <tr><td>{openingLabel(o.kind)}</td><td class="v">{a2(o.area)}</td></tr>
              {/each}
            </tbody>
          </table>
        {/if}

        {#if part.faces.length > 1}
          <div class="sum">
            Σ {part.faces.map((f) => num(f.area)).concat((part.clad ?? []).map((c) => num(c[1]))).join(" + ")} = <b>{a2(part.roof_area)}</b>
          </div>
        {/if}
      </section>
    {/each}

    {#if data.manual_faces.length}
      <section>
        <h4><span>{t("bd.manualFaces")}</span></h4>
        {#each data.manual_faces as f}
          <div class="face">
            <div class="face-head">
              <span class="no">{faceRef(null, f.no)}</span>
              <span>{f.label || t("manual.faceN", { n: f.no })}</span>
              <b>{a2(f.area)}</b>
            </div>
            <div class="calc">
              <span>{fixed(f.plan_area, 3)} m² ÷ cos {deg(f.pitch_deg)} = {fixed(f.plan_area, 3)} × {fixed(f.factor, 4)} = {a2(f.area)}</span>
            </div>
          </div>
        {/each}
      </section>
    {/if}

    {#if multiMaterial || (data.materials.length === 1 && data.materials[0].material !== 1)}
      <section>
        <h4><span>{t("res.byMaterial")}</span></h4>
        {#each data.materials as mt}
          <div class="sum">
            <span class="mat-name">{materialName(project, mt.material)}</span>
            {mt.items.map((i) => `${faceRef(i.part, i.face)}: ${num(i.area)}`).join(" + ")} = <b>{a2(mt.area)}</b>
          </div>
        {/each}
      </section>
    {/if}

    <section>
      <h4><span>{t("res.lines")}</span></h4>
      {#each data.lines as g}
        <details class="lines">
          <summary>
            <code>{g.code}</code>
            <span>{lineLabel(g.code)} <span class="n">×{g.items.length}</span></span>
            <b>{len(g.total)}</b>
          </summary>
          <table>
            <tbody>
              {#each g.items as l, li}
                <tr
                  onpointerenter={() => onmark?.({ line: [[l.a[0], l.a[1]], [l.b[0], l.b[1]]] })}
                  onpointerleave={() => onmark?.(null)}
                >
                  <td class="no">{li + 1}</td>
                  <td class="delta">
                    {#if l.typed}
                      {t("bd.typed")}
                    {:else}
                      √({num(l.delta[0])}² + {num(l.delta[1])}² + {num(l.delta[2])}²){#if l.part === null}<span class="tag">{t("bd.manual")}</span>{/if}
                    {/if}
                  </td>
                  <td class="v">{num(l.length)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          <div class="sum">
            Σ {num(g.items.reduce((s, l) => s + l.length, 0))}{#if g.deducted}
              − {num(g.deducted)} <em>{t("bd.lineDeducted")}</em>{/if} = <b>{len(g.total)}</b>
          </div>
        </details>
      {/each}
    </section>

    <section class="total">
      <h4><span>{t("bd.total")}</span><b>{a2(data.roof_area)}</b></h4>
      {#if data.parts.length + data.manual_faces.length > 1}
        <div class="sum">
          Σ {data.parts.map((p) => num(p.roof_area)).concat(data.manual_faces.map((f) => num(f.area))).join(" + ")} = <b>{a2(data.roof_area)}</b>
        </div>
      {/if}
      <p class="check" class:bad={!agrees}>
        <i></i>{agrees ? t("bd.agrees") : t("bd.differs", { v: result ? a2(result.totals.roof_area) : "—" })}
      </p>
    </section>
  {/if}
</div>

<style>
  .bd {
    display: flex;
    flex-direction: column;
    gap: 12px;
    font-size: 12.5px;
  }
  .intro,
  .muted {
    margin: 0;
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.45;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h4 {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin: 0;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  h4.part {
    font-size: 13px;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text);
  }
  h4 b {
    font-variant-numeric: tabular-nums;
    color: var(--text);
    text-transform: none;
    letter-spacing: 0;
  }
  summary {
    cursor: pointer;
    color: var(--text-2);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }
  th {
    padding: 4px 2px;
    font-weight: 500;
    color: var(--text-3);
    text-align: right;
    font-size: 11px;
  }
  td {
    padding: 3px 2px;
    border-bottom: 1px solid var(--border);
  }
  .walls td {
    text-align: right;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
  }
  .walls th:first-child,
  .walls td:first-child {
    text-align: left;
    color: var(--text-3);
  }
  .tag {
    margin-left: 4px;
    color: var(--text-3);
    font-family: inherit;
    font-size: 10.5px;
  }
  .face {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .face:hover {
    border-color: var(--accent);
  }
  .face-head {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .face-head b {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }
  .no {
    min-width: 18px;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-weight: 700;
    color: var(--text-2);
  }
  .mat {
    color: var(--text-3);
  }
  .calc {
    display: flex;
    flex-direction: column;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .calc em,
  .sum em {
    font-style: normal;
    color: var(--text-3);
    font-family: system-ui, sans-serif;
  }
  .calc .note {
    color: var(--text-3);
    font-family: system-ui, sans-serif;
  }
  .corners summary {
    font-size: 11px;
    color: var(--text-3);
  }
  .sub {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .v {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .sum {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  .sum b {
    color: var(--text);
    white-space: nowrap;
  }
  .mat-name {
    display: block;
    font-family: system-ui, sans-serif;
    color: var(--text);
  }
  details.lines summary {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
    color: var(--text);
  }
  details.lines summary b {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }
  details.lines code {
    min-width: 22px;
    font-weight: 700;
  }
  details.lines .n {
    color: var(--text-3);
    font-size: 11.5px;
  }
  details.lines tr:hover td {
    background: var(--accent-soft);
  }
  .delta {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11px;
    color: var(--text-2);
  }
  details.lines .sum {
    padding: 4px 0 8px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    color: var(--text-2);
    font-size: 11.5px;
  }
  .check i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--text-3);
  }
  .check.bad i {
    background: var(--danger);
  }
  .err {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .err i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--danger);
  }
  .err code {
    margin-left: auto;
    font-size: 11px;
    color: var(--text-3);
  }
</style>
