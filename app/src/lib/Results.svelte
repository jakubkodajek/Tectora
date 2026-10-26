<script lang="ts">
  import { m, m2, num } from "./format";
  import type { RoofQuantities } from "./types";

  let { totals }: { totals: RoofQuantities | null } = $props();

  const rows = $derived(
    totals
      ? [
          { color: "var(--line-eave)", label: "Okapy", value: m(totals.eave_length) },
          { color: "var(--line-ridge)", label: "Hřebeny", value: m(totals.ridge_length) },
          { color: "var(--line-hip)", label: `Nároží (${totals.hip_count})`, value: m(totals.hip_length) },
          { color: "var(--line-valley)", label: `Úžlabí (${totals.valley_count})`, value: m(totals.valley_length) },
          { color: "var(--line-verge)", label: "Štítové hrany", value: m(totals.verge_length) },
          { color: "var(--line-abutment)", label: "Napojení na zeď", value: m(totals.abutment_length) },
        ]
      : [],
  );
</script>

<div class="results">
  <div class="hero">
    <span class="muted">Plocha střechy</span>
    <strong>{totals ? m2(totals.roof_area) : "—"}</strong>
    {#if totals}<span class="muted small">Výška hřebene {num(totals.ridge_height)} m</span>{/if}
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
