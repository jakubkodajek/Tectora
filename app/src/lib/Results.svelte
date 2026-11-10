<script lang="ts">
  // Results in DACAD's terms: roof areas per part and line lengths by
  // DACAD line code (T eave, F ridge, G hip, K valley, O verge, P shed top edge, c pitch break).
  import { figureLabel, formatFigure } from "./changes";
  import { m, m2, num } from "./format";
  import { numberLocale, t } from "./i18n.svelte";
  import { materialName } from "./materials";
  import { areaRows, joinedWith, warningText } from "./results";
  import { partLabel } from "./partname";
  import type { CalcResult, Project } from "./types";

  let {
    result,
    project = null,
    onstore,
    blocked = null,
  }: {
    result: CalcResult | null;
    project?: Project | null;
    onstore?: () => void;
    /** Why the figures cannot become the offer's stored result now (the core gave no roof), or null. */
    blocked?: string | null;
  } = $props();

  /** The stored result (figures of the offer) against the current one. */
  const stored = $derived(result?.stored_result ?? null);
  const diffLabel = (key: string) => figureLabel(key, project);
  const fmtDiff = formatFigure;
  /** Per part, the parts the calculation joined its roof with (named under the part's area). */
  const joins = $derived(joinedWith(result));
  /** Parts and hand-drawn faces with their areas: everything the total counts (UX-009). */
  const areas = $derived(areaRows(result));

  /** Area per material, shown once the roof has more than one material or a non-default one. */
  const materials = $derived(
    (result?.totals.area_by_material ?? []).filter(([, a]) => Math.abs(a) >= 0.005),
  );

  const totals = $derived(result?.totals ?? null);

  /** Plausibility warnings of every part, with the part's name when there are several. */
  const warnings = $derived(
    (result?.parts ?? []).flatMap((p, i) =>
      (p.geometry.warnings ?? []).map((w) => ({ part: result!.parts.length > 1 ? partLabel(p.name, i) : null, text: warningText(w) })),
    ),
  );
  const rows = $derived(
    totals
      ? [
          { code: "T", color: "var(--line-eave)", label: t("res.eave"), value: totals.eave_length, n: null },
          { code: "F", color: "var(--line-ridge)", label: t("res.ridge"), value: totals.ridge_length, n: null },
          { code: "G", color: "var(--line-hip)", label: t("res.hipShort"), value: totals.hip_length, n: totals.hip_count },
          { code: "K", color: "var(--line-valley)", label: t("res.valleyShort"), value: totals.valley_length, n: totals.valley_count },
          { code: "O", color: "var(--line-verge)", label: t("res.verge"), value: totals.verge_length, n: null },
          { code: "P", color: "var(--line-pult)", label: t("res.pult"), value: totals.pult_length, n: null },
          { code: "c", color: "var(--line-knick)", label: t("res.knick"), value: totals.knick_length, n: null },
          { code: "S", color: "var(--line-gorge)", label: t("res.gorge"), value: totals.gorge_length ?? 0, n: null },
          { code: "W", color: "var(--line-abutment)", label: t("res.abutment"), value: totals.abutment_length, n: null },
        ].filter((r) => r.value >= 0.005 || !["P", "c", "S", "W"].includes(r.code))
      : [],
  );
</script>

<div class="results">
  <div class="hero">
    <span>{t("res.area")}</span>
    <strong>{totals ? m2(totals.roof_area) : "—"}</strong>
    {#if totals}<small>{t("res.ridgeHeight", { v: num(totals.ridge_height) })}</small>{/if}
  </div>

  {#if warnings.length}
    <div class="warn" role="status">
      <div class="warn-head"><i></i>{t("res.warn.title")}</div>
      {#each warnings as w}
        <!-- The space is an expression: Svelte trims plain whitespace at the end of a block. -->
        <p>{#if w.part}<code>{w.part}</code>{" "}{/if}{w.text}</p>
      {/each}
    </div>
  {/if}

  {#if result && areas.length > 1}
    <h4>{t("res.areas")}</h4>
    <table class="areas">
      <tbody>
        {#each areas as a}
          <tr>
            <td>
              {a.name}
              {#if a.manual}
                <small class="joined">{t("bd.manual")}</small>
              {:else if joins[a.part]?.length}
                <small class="joined">{t("res.joinedTo", { names: joins[a.part].map((j) => (result.parts[j] ? partLabel(result.parts[j].name, j) : "")).join(", ") })}</small>
              {/if}
            </td>
            <td class="v">{m2(a.area)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if materials.length > 1 || (materials.length === 1 && materials[0][0] !== 1)}
    <h4>{t("res.byMaterial")}</h4>
    <table>
      <tbody>
        {#each materials as [mat, a]}
          <tr><td>{materialName(project, mat)}</td><td class="v">{m2(a)}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}

  <h4>{t("res.lines")}</h4>
  <table>
    <tbody>
      {#each rows as r}
        <tr class:zero={r.value < 0.005}>
          <td class="code"><i style="background:{r.color}"></i>{r.code}</td>
          <td>{r.label}{#if r.n}<span class="n">×{r.n}</span>{/if}</td>
          <td class="v">{m(r.value)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="note">{t("res.note")}</p>

  {#if stored}
    <h4>{t("snap.title")}</h4>
    <div class="snap" class:changed={stored.differences.length > 0}>
      <div class="snap-head">
        <i class="sq"></i>
        <span>
          {stored.differences.length ? t("snap.changed") : t("snap.same")}
          · {new Date(stored.stored.taken_ms).toLocaleDateString(numberLocale())}
        </span>
      </div>
      {#if stored.differences.length}
        <table class="diff">
          <tbody>
            {#each stored.differences as d}
              <tr><td>{diffLabel(d.key)}</td><td class="v">{fmtDiff(d.key, d.stored)}</td><td class="v">{fmtDiff(d.key, d.current)}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if stored.engine_changed}
        <p class="mono">{t("snap.engine", { v: stored.stored.engine_version })}</p>
      {/if}
      {#if stored.differences.length && onstore}
        <button class="btn sm" disabled={!!blocked} title={blocked ?? undefined} onclick={onstore}>{t("snap.store")}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .joined {
    display: block;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .results {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hero {
    display: flex;
    flex-direction: column;
    padding: 14px;
    border-radius: 10px;
    background: var(--accent-soft);
  }
  .hero span,
  .hero small {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .hero strong {
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  h4 {
    margin: 6px 0 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-3);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  td {
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  tr.zero {
    color: var(--text-3);
  }
  .code {
    width: 44px;
    font-weight: 700;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  }
  .code i {
    display: inline-block;
    width: 10px;
    height: 3px;
    margin-right: 8px;
    border-radius: 2px;
    vertical-align: middle;
  }
  .n {
    margin-left: 5px;
    color: var(--text-3);
    font-size: 12px;
  }
  .v {
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
  .warn {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 12.5px;
  }
  .warn-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
  }
  .warn-head i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--ai);
  }
  .warn p {
    margin: 6px 0 0 16px;
    color: var(--text-2);
  }
  .warn code {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
  }
  .snap {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    font-size: 12.5px;
  }
  .snap-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sq {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--text-3);
  }
  .snap.changed .sq {
    background: var(--danger);
  }
  .diff td {
    padding: 4px 0;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 12px;
  }
  .mono {
    margin: 0;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .btn.sm {
    align-self: flex-start;
  }
  .note {
    margin: 0;
    font-size: 11.5px;
    color: var(--text-3);
  }
</style>
