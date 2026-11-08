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
  import Modal from "./Modal.svelte";
  import BreakdownDetail from "./BreakdownDetail.svelte";
  import type { BreakdownDetail as Detail, CalcBreakdown, CalcResult, PartDeductions, PlanMark, Project } from "./types";

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
  /** The face, gable or line open in the detailed dialog, with its title and its part's deductions. */
  let open = $state<{ title: string; detail: Detail; deductions: PartDeductions | null } | null>(null);
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

  const MINUS = "\u2212";
  /** Fixed decimals, one minus sign (U+2212) and no grouping, as in the detailed dialog. */
  const fixed = (v: number, d: number) => {
    const s = new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: d, maximumFractionDigits: d, useGrouping: false })
      .format(Object.is(v, -0) ? 0 : v)
      .replace(/^[-\u2212]/, MINUS);
    return /^−0([.,]0+)?$/.test(s) ? s.slice(1) : s;
  };
  const a2 = (v: number) => `${num(v)} m²`;
  const len = (v: number) => `${num(v)} m`;
  /** A sum's sign and decimals: digits 0 means it adds up only to float noise. */
  const sumFmt = (digits: number) => ({ d: digits || (data?.decimals.sum_max ?? 12), eq: digits ? "=" : "≈" });
  /** Terms of a sum as written: "a + b − c". */
  const terms = (vals: number[], d: number) =>
    vals.map((v, i) => (i === 0 ? fixed(v, d) : v < 0 ? `${MINUS} ${fixed(-v, d)}` : `+ ${fixed(v, d)}`)).join(" ");

  function partDeductions(part: CalcBreakdown["parts"][number]): PartDeductions {
    const items = (part.openings ?? []).map((o) => ({ label: openingLabel(o.kind), area: o.area }));
    if (part.other_cuts) items.push({ label: t("bd.otherCuts"), area: part.other_cuts });
    return { items, total: part.deducted };
  }
  /** Degrees with up to `max` decimals, trailing zeros trimmed. */
  const deg = (v: number, max = 4) =>
    `${new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 0, maximumFractionDigits: max, useGrouping: false }).format(v)}°`;

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

  {#if open && data}
    <Modal title={open.title} width={760} onclose={() => (open = null)}>
      <BreakdownDetail detail={open.detail} decimals={data.decimals} deductions={open.deductions} />
    </Modal>
  {/if}

  {#if error}
    <div class="err" role="status"><i></i><span>{t("bd.error")}</span><code>{error}</code></div>
  {:else if !data}
    <p class="muted">…</p>
  {:else if !data.parts.length && !data.manual_faces.length}
    <p class="muted">{t("bd.empty")}</p>
  {:else}
    {@const D = data.decimals}
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
                  <td>{fixed(w.length, 3)}</td>
                  <td>
                    {w.gable ? t("edge.gable") : deg(w.pitch_deg)}{#if w.half_hip}<span class="tag">{t("bd.halfHip")}</span>{/if}{#if w.breaks}<span class="tag">{t("bd.breaks", { n: w.breaks })}</span>{/if}{#if w.clad}<span class="tag">{t("bd.clad")}</span>{/if}
                  </td>
                  <td>{fixed(w.eave_height, 3)}</td>
                  <td>{fixed(w.overhang, 3)}</td>
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
          <button
            class="face"
            title={t("bd.open")}
            onclick={() => (open = { title: t("bdd.faceTitle", { n: faceRef(pi, f.no) }), detail: { kind: "face", face: f }, deductions: partDeductions(part) })}
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
              {#if f.engine_gross === undefined && !f.vertical}
                <span>{fixed(f.plan_area, D.plan)} m² ÷ cos {deg(f.pitch_deg, 6)}{#if f.deducted > 0}{" "}{MINUS} {fixed(f.deducted, f.digits)} m²{/if}</span>
              {:else}
                <span>{t("bd.engineShort", { v: fixed(f.engine_gross ?? f.gross, f.digits) })}{#if f.deducted > 0}{" "}{MINUS} {fixed(f.deducted, f.digits)} m²{/if}</span>
              {/if}
              <span>= {fixed(f.area, f.digits)} m²</span>
            </div>
            <span class="more">{t("bd.open")}</span>
          </button>
        {/each}

        {#each part.clad_faces ?? [] as c}
          <button class="face" title={t("bd.open")} onclick={() => (open = { title: t("bdd.cladTitle", { n: c.wall ?? "?" }), detail: { kind: "clad", clad: c }, deductions: null })}>
            <div class="face-head">
              <span class="no">{faceRef(pi, null)}</span>
              <span>{t("bd.cladGable", { n: c.wall ?? "?" })}</span>
              {#if multiMaterial}<span class="mat">{materialName(project, c.material)}</span>{/if}
              <b>{a2(c.area)}</b>
            </div>
            <span class="more">{t("bd.open")}</span>
          </button>
        {/each}

        {#if part.deducted > 0}
          {@const ded = partDeductions(part)}
          <div class="sub">{t("bd.openings")}</div>
          <table>
            <tbody>
              {#each ded.items as o}
                <tr><td>{o.label}</td><td class="v">{fixed(o.area, 3)} m²</td></tr>
              {/each}
            </tbody>
          </table>
        {/if}

        {#if part.faces.length + (part.clad_faces?.length ?? 0) > 1}
          {@const sf = sumFmt(part.digits)}
          <div class="sum">
            Σ {terms(part.faces.map((f) => f.area).concat((part.clad_faces ?? []).map((c) => c.area)), sf.d)} {sf.eq} <b>{fixed(part.roof_area, sf.d)}</b> ≐ <b>{a2(part.roof_area)}</b>
          </div>
        {/if}
      </section>
    {/each}

    {#if data.manual_faces.length}
      <section>
        <h4><span>{t("bd.manualFaces")}</span></h4>
        {#each data.manual_faces as f}
          <button class="face" onclick={() => (open = { title: f.label || t("manual.faceN", { n: f.no }), detail: { kind: "manual", face: f }, deductions: null })}>
            <div class="face-head">
              <span class="no">{faceRef(null, f.no)}</span>
              <span>{f.label || t("manual.faceN", { n: f.no })}</span>
              <b>{a2(f.area)}</b>
            </div>
            <div class="calc">
              {#if f.pitch_out_of_range}
                <span>{deg(f.pitch_deg, 6)}</span>
              {:else}
                <span>{fixed(f.plan_area, D.plan)} m² ÷ cos {deg(f.pitch_deg, 6)}</span>
              {/if}
              <span>= {fixed(f.area, f.digits)} m²</span>
            </div>
            <span class="more">{t("bd.open")}</span>
          </button>
        {/each}
      </section>
    {/if}

    {#if multiMaterial || (data.materials.length === 1 && data.materials[0].material !== 1)}
      <section>
        <h4><span>{t("res.byMaterial")}</span></h4>
        {#each data.materials as mt}
          {@const sf = sumFmt(mt.digits)}
          <div class="sum">
            <span class="mat-name">{materialName(project, mt.material)}</span>
            {mt.items.map((i, k) => `${k ? "+ " : ""}${faceRef(i.part, i.face)}: ${fixed(i.area, sf.d)}`).join(" ")} {sf.eq} <b>{fixed(mt.area, sf.d)}</b> ≐ <b>{a2(mt.area)}</b>
          </div>
        {/each}
      </section>
    {/if}

    <section>
      <h4><span>{t("res.lines")}</span></h4>
      {#each data.lines as g}
        {@const sf = sumFmt(g.digits)}
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
                  class="clickable"
                  title={t("bd.open")}
                  onclick={() => (open = { title: t("bdd.lineTitle", { code: g.code, label: lineLabel(g.code), n: li + 1 }), detail: { kind: "line", code: g.code, line: l }, deductions: null })}
                  onpointerenter={() => onmark?.({ line: [[l.a[0], l.a[1]], [l.b[0], l.b[1]]] })}
                  onpointerleave={() => onmark?.(null)}
                >
                  <td class="no">{li + 1}</td>
                  <td class="delta">
                    {#if l.typed}{t("bd.typed")}{:else}{t("bd.lineOpen")}{/if}{#if l.part === null}<span class="tag">{t("bd.manual")}</span>{/if}
                  </td>
                  <td class="v">{fixed(l.length, sf.d)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          <div class="sum">
            Σ {terms(g.items.map((l) => l.length).concat(g.deducted ? [-g.deducted] : []), sf.d)}{#if g.deducted} <em>({t("bd.lineDeducted")})</em>{/if}
            {sf.eq} <b>{fixed(g.sum, sf.d)}</b> ≐ <b>{len(g.total)}</b>
          </div>
        </details>
      {/each}
    </section>

    <section class="total">
      <h4><span>{t("bd.total")}</span><b>{a2(data.roof_area)}</b></h4>
      {#if data.parts.length + data.manual_faces.length > 1}
        {@const sf = sumFmt(data.digits)}
        <div class="sum">
          Σ {terms(data.parts.map((p) => p.roof_area).concat(data.manual_faces.map((f) => f.area)), sf.d)} {sf.eq} <b>{fixed(data.roof_area, sf.d)}</b> ≐ <b>{a2(data.roof_area)}</b>
        </div>
      {/if}
      <p class="check" class:bad={!agrees}>
        <i></i>{agrees ? t("bd.agrees") : t("bd.differs", { v: result ? a2(result.totals.roof_area) : "—" })}
      </p>
      <p class="muted">{t("bd.sumNote")}</p>
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
  button.face {
    width: 100%;
    font: inherit;
    color: inherit;
    text-align: left;
    background: none;
    cursor: pointer;
  }
  .face:hover,
  .face:focus-visible {
    border-color: var(--accent);
  }
  .more {
    align-self: flex-end;
    font-size: 11px;
    color: var(--text-3);
  }
  .face:hover .more {
    color: var(--accent);
  }
  tr.clickable {
    cursor: pointer;
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
  .sum em {
    font-style: normal;
    color: var(--text-3);
    font-family: system-ui, sans-serif;
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
