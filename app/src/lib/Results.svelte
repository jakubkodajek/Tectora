<script lang="ts">
  // Results in DACAD's terms: roof areas per part and line lengths by
  // DACAD line code (T eave, F ridge, G hip, K valley, O verge, P shed top edge, c pitch break).
  import { m, m2, num } from "./format";
  import { t } from "./i18n.svelte";
  import type { CalcResult } from "./types";

  let { result }: { result: CalcResult | null } = $props();

  const totals = $derived(result?.totals ?? null);
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
          { code: "W", color: "var(--line-abutment)", label: t("res.abutment"), value: totals.abutment_length, n: null },
        ].filter((r) => r.value >= 0.005 || !["P", "c", "W"].includes(r.code))
      : [],
  );
</script>

<div class="results">
  <div class="hero">
    <span>{t("res.area")}</span>
    <strong>{totals ? m2(totals.roof_area) : "—"}</strong>
    {#if totals}<small>{t("res.ridgeHeight", { v: num(totals.ridge_height) })}</small>{/if}
  </div>

  {#if result && result.parts.length > 1}
    <h4>{t("res.areas")}</h4>
    <table>
      <tbody>
        {#each result.parts as p}
          <tr><td>{p.name}</td><td class="v">{m2(p.quantities.roof_area)}</td></tr>
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
          <td>{r.label}{#if r.n}<span class="n"> ×{r.n}</span>{/if}</td>
          <td class="v">{m(r.value)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="note">{t("res.note")}</p>
</div>

<style>
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
    color: var(--text-3);
  }
  .v {
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
  .note {
    margin: 0;
    font-size: 11.5px;
    color: var(--text-3);
  }
</style>
