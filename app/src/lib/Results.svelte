<script lang="ts">
  import { m, m2, num } from "./format";
  import type { RoofQuantities } from "./types";
  import { t } from "./i18n.svelte";

  let { totals }: { totals: RoofQuantities | null } = $props();

  const rows = $derived(
    totals
      ? [
          { color: "var(--line-eave)", label: t("res.eave"), value: m(totals.eave_length) },
          { color: "var(--line-ridge)", label: t("res.ridge"), value: m(totals.ridge_length) },
          { color: "var(--line-hip)", label: t("res.hip", { n: totals.hip_count }), value: m(totals.hip_length) },
          { color: "var(--line-valley)", label: t("res.valley", { n: totals.valley_count }), value: m(totals.valley_length) },
          { color: "var(--line-verge)", label: t("res.verge"), value: m(totals.verge_length) },
          { color: "var(--line-abutment)", label: t("res.abutment"), value: m(totals.abutment_length) },
        ]
      : [],
  );
</script>

<div class="results">
  <div class="hero">
    <span class="muted">{t("res.area")}</span>
    <strong>{totals ? m2(totals.roof_area) : "—"}</strong>
    {#if totals}<span class="muted small">{t("res.ridgeHeight", { v: num(totals.ridge_height) })}</span>{/if}
  </div>
  <table>
    <tbody>
      {#each rows as r}
        <tr>
          <td><i style="background:{r.color}"></i>{r.label}</td>
          <td class="v">{r.value}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .results {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .hero {
    display: flex;
    flex-direction: column;
    padding: 14px;
    border-radius: 10px;
    background: var(--accent-soft);
  }
  .hero strong {
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  .small {
    font-size: 12px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  td {
    padding: 7px 0;
    border-bottom: 1px solid var(--border);
  }
  td i {
    display: inline-block;
    width: 10px;
    height: 3px;
    margin-right: 10px;
    border-radius: 2px;
    vertical-align: middle;
  }
  .v {
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
</style>
