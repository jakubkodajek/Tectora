<script lang="ts">
  // One face, clad gable or line of the calculation breakdown, step by
  // step, so it can be redone by hand with a calculator. Every number is
  // the core's value rounded to the decimals the core gives (`decimals`,
  // and `digits` per item for the last steps); nothing is calculated here.
  import { numberLocale, t } from "./i18n.svelte";
  import type { BreakdownDecimals, BreakdownDetail as Detail, PartDeductions } from "./types";

  let { detail, decimals: D, deductions = null }: { detail: Detail; decimals: BreakdownDecimals; deductions?: PartDeductions | null } = $props();

  const MINUS = "−";
  const fmt = (v: number, min: number, max: number) => {
    const s = new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: min, maximumFractionDigits: max, useGrouping: false }).format(
      Object.is(v, -0) ? 0 : v,
    );
    // One minus sign for negative numbers and for the operator; no "−0".
    const m = s.replace(/^[-−]/, MINUS);
    return /^−0([.,]0+)?$/.test(m) ? m.slice(1) : m;
  };
  /** Fixed decimals. */
  const f = (v: number, d: number) => fmt(v, d, d);
  /** Up to `d` decimals, trailing zeros trimmed to 3. */
  const tr = (v: number, d: number) => fmt(v, Math.min(3, d), d);
  /** A negative number in brackets, as written after an operator. */
  const br = (s: string) => (s.startsWith(MINUS) ? `(${s})` : s);
  const coord = (v: number) => tr(v, D.coord);
  const pitch = (v: number) => tr(v, D.pitch);
  const sub = (i: number) => String(i + 1);
</script>

{#snippet shoelace(pts: number[][], products: [number, number][], cross: number, a: string, b: string)}
  {@const n = pts.length}
  <table>
    <tbody>
      {#each products as p, i}
        {@const j = (i + 1) % n}
        <tr>
          <td class="i">{sub(i)}→{sub(j)}</td>
          <td class="l">{br(coord(pts[i][0]))} · {br(coord(pts[j][1]))} {MINUS} {br(coord(pts[j][0]))} · {br(coord(pts[i][1]))}</td>
          <td>= {br(f(p[0], D.product))} {MINUS} {br(f(p[1], D.product))}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="step">Σ = {f(cross, D.plan)} m²</p>
  <p class="step">{a} = |{f(cross, D.plan)}| / 2 = <b>{b}</b></p>
{/snippet}

<div class="detail">
  {#if detail.kind === "face" || detail.kind === "manual"}
    {@const face = detail.face}
    {@const corners = detail.kind === "face" ? detail.face.polygon.map((p) => [p[0], p[1], p[2]]) : detail.face.points.map((p) => [p[0], p[1]])}
    {@const d = face.digits}
    <section>
      <h5>1. {t("bdd.corners")}</h5>
      <table>
        <thead>
          <tr><th class="i">#</th><th>x</th><th>y</th>{#if detail.kind === "face"}<th>{t("bdd.height")}</th>{/if}</tr>
        </thead>
        <tbody>
          {#each corners as c, i}
            <tr><td class="i">{sub(i)}</td><td>{coord(c[0])}</td><td>{coord(c[1])}</td>{#if c.length > 2}<td>{coord(c[2])}</td>{/if}</tr>
          {/each}
        </tbody>
      </table>
    </section>

    <section>
      <h5>2. {t("bdd.plan")}</h5>
      <p class="formula">S = ½ · | Σ (xᵢ · yᵢ₊₁ {MINUS} xᵢ₊₁ · yᵢ) |</p>
      <p class="hint">{t("bdd.planHint", { n: corners.length })}</p>
      {@render shoelace(corners, face.products, face.cross_sum, "S", `${f(face.plan_area, D.plan)} m²`)}
    </section>

    {@const vertical = detail.kind === "face" && !!detail.face.vertical}
    {@const out = detail.kind === "manual" && !!detail.face.pitch_out_of_range}
    <section>
      <h5>3. {t("bdd.pitch")}</h5>
      <p class="step">α = {pitch(face.pitch_deg)}°</p>
      <p class="step">cos α = cos {pitch(face.pitch_deg)}° = {f(face.cos_pitch, face.cos_digits)}</p>
      {#if vertical}
        <p class="hint">{t("bdd.vertical")}</p>
      {:else if out}
        <p class="hint">{t("bdd.manualOut", { p: pitch(face.pitch_deg) })}</p>
      {:else if face.cos_pitch < 0}
        <p class="step">1 / |cos α| = 1 / {f(-face.cos_pitch, face.cos_digits)} = <b>{f(face.factor, D.factor)}</b></p>
      {:else}
        <p class="step">1 / cos α = 1 / {f(face.cos_pitch, face.cos_digits)} = <b>{f(face.factor, D.factor)}</b></p>
      {/if}
    </section>

    <section>
      <h5>4. {t("bdd.sloped")}</h5>
      {#if vertical}
        <p class="step">A = <b>{f(detail.face.gross, d)} m²</b></p>
      {:else if out}
        <p class="step">A = <b>{f(0, d)} m²</b></p>
      {:else}
        <p class="formula">{face.cos_pitch < 0 ? "A = S / |cos α| = S · (1 / |cos α|)" : "A = S / cos α = S · (1 / cos α)"}</p>
      {/if}
      {#if vertical || out}
        <!-- no formula -->
      {:else if detail.kind === "face"}
        {@const ff = detail.face}
        <p class="step">A = {f(ff.plan_area, D.plan)} · {f(ff.factor, D.factor)} = <b>{f(ff.gross, d)} m²</b></p>
        {@const diff3d = ff.area_3d - ff.gross}
        {#if Math.abs(diff3d) < 0.0005}
          <p class="check"><i class="ok">✓</i>{t("bdd.check3d", { v: f(ff.area_3d, d) })}</p>
        {:else if ff.off_plane < 0.001 && Math.abs(diff3d) <= 1e-4 * ff.gross}
          <!-- Corners in one plane; the core's single-precision coordinates tilt it slightly. -->
          <p class="check"><i class="ok">✓</i>{t("bdd.check3dRound", { v: f(ff.area_3d, 3), d: f(diff3d, 3) })}</p>
        {:else}
          <p class="check"><i class="warn"></i>{t("bdd.check3dOff", { v: f(ff.area_3d, 3), mm: tr(ff.off_plane * 1000, 1) })}</p>
        {/if}
        {#if ff.engine_gross !== undefined}
          <p class="check"><i class="warn"></i>{t("bdd.engine", { v: f(ff.engine_gross, d), diff: f(ff.engine_gross - ff.gross, d) })}</p>
        {/if}
      {:else}
        <p class="step">A = {f(face.plan_area, D.plan)} · {f(face.factor, D.factor)} = <b>{f(face.area, d)} m²</b></p>
      {/if}
    </section>

    {#if detail.kind === "face"}
      {@const ff = detail.face}
      <section>
        <h5>5. {t("bdd.deduct")}</h5>
        {#if ff.deducted > 0}
          <p class="step">{f(ff.engine_gross ?? ff.gross, d)} {MINUS} {f(ff.deducted, d)} = <b>{f(ff.area, d)} m²</b></p>
          {#if deductions && deductions.items.length}
            <p class="hint">
              {t("bdd.deductHint", {
                list: deductions.items.map((x) => `${x.label} ${f(x.area, 3)}`).join(" + "),
                total: f(deductions.total, 3),
              })}
            </p>
          {/if}
        {:else}
          <p class="hint">{t("bdd.noDeduct")}</p>
        {/if}
      </section>
    {/if}

    <p class="result">{t("bdd.result")} <b>{f(face.area, 2)} m²</b></p>
  {:else if detail.kind === "clad"}
    {@const c = detail.clad}
    <p class="hint">{t("bdd.cladHint")}</p>
    {#each c.pieces as piece, k}
      <section>
        <h5>{c.pieces.length > 1 ? `${t("bdd.piece", { n: k + 1 })}: ` : ""}{t("bdd.cladCorners")}</h5>
        <table>
          <thead><tr><th class="i">#</th><th>x</th><th>y</th><th>{t("bdd.height")}</th><th>u</th></tr></thead>
          <tbody>
            {#each piece.polygon as p, i}
              <tr><td class="i">{sub(i)}</td><td>{coord(p[0])}</td><td>{coord(p[1])}</td><td>{coord(p[2])}</td><td>{coord(piece.uz[i][0])}</td></tr>
            {/each}
          </tbody>
        </table>
        <p class="formula">A = ½ · | Σ (uᵢ · zᵢ₊₁ {MINUS} uᵢ₊₁ · zᵢ) |</p>
        {@render shoelace(piece.uz, piece.products, piece.cross_sum, "A", `${f(piece.area, c.digits)} m²`)}
      </section>
    {/each}
    {#if c.formula_area !== undefined}
      <p class="check"><i class="warn"></i>{t("bdd.cladCore", { v: f(c.area, c.digits), s: f(c.formula_area, c.digits) })}</p>
    {/if}
    <p class="result">{t("bdd.result")} <b>{f(c.area, 2)} m²</b></p>
  {:else}
    {@const l = detail.line}
    {#if l.typed}
      <p class="hint">{t("bdd.typed")}</p>
      <p class="result">{t("bdd.length")}: <b>{f(l.length, 2)} m</b></p>
    {:else}
      <section>
        <h5>1. {t("bdd.ends")}</h5>
        <table>
          <thead><tr><th class="i"></th><th>x</th><th>y</th><th>{t("bdd.height")}</th></tr></thead>
          <tbody>
            <tr><td class="i">A</td><td>{coord(l.a[0])}</td><td>{coord(l.a[1])}</td><td>{coord(l.a[2])}</td></tr>
            <tr><td class="i">B</td><td>{coord(l.b[0])}</td><td>{coord(l.b[1])}</td><td>{coord(l.b[2])}</td></tr>
          </tbody>
        </table>
      </section>
      <section>
        <h5>2. {t("bdd.diff")}</h5>
        {#each ["x", "y", "z"] as axis, k}
          <p class="step">Δ{axis} = {coord(l.b[k])} {MINUS} {br(coord(l.a[k]))} = {coord(l.delta[k])}; &nbsp; Δ{axis}² = {br(coord(l.delta[k]))}² = {f(l.squares[k], l.sq_digits)}</p>
        {/each}
      </section>
      <section>
        <h5>3. {t("bdd.length")}</h5>
        <p class="formula">L = √(Δx² + Δy² + Δz²)</p>
        <p class="step">L = √({f(l.squares[0], l.sq_digits)} + {f(l.squares[1], l.sq_digits)} + {f(l.squares[2], l.sq_digits)}) = √{f(l.sum_sq, l.sq_digits)} = <b>{f(l.length, l.digits)} m</b></p>
      </section>
      <p class="result">{t("bdd.result")} <b>{f(l.length, 2)} m</b></p>
    {/if}
  {/if}
  <p class="note">{t("bdd.rounding")}</p>
</div>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: 14px;
    font-size: 13px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h5 {
    margin: 0 0 2px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  table {
    border-collapse: collapse;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  th {
    padding: 3px 10px;
    font-weight: 500;
    color: var(--text-3);
    text-align: right;
  }
  td {
    padding: 3px 10px;
    border-bottom: 1px solid var(--border);
    text-align: right;
    white-space: nowrap;
  }
  th.i,
  td.i {
    text-align: left;
    color: var(--text-3);
  }
  td.l {
    text-align: left;
    color: var(--text-2);
  }
  .formula,
  .step {
    margin: 0;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 12.5px;
    overflow-wrap: anywhere;
  }
  .formula {
    color: var(--text-2);
  }
  .hint,
  .note {
    margin: 0;
    color: var(--text-3);
    font-size: 12px;
  }
  .check {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0;
    color: var(--text-2);
    font-size: 12px;
  }
  .check i {
    flex: none;
    font-style: normal;
    font-weight: 700;
  }
  .check i.warn {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--ai);
  }
  .result {
    margin: 0;
    padding-top: 8px;
    border-top: 1px solid var(--border);
    font-size: 14px;
  }
  .result b {
    font-variant-numeric: tabular-nums;
  }
</style>
