<script lang="ts">
  // The top-bar button of the bug report: icon and label, the label hidden
  // at the compact breakpoint (the icon keeps the name for screen readers).
  // Beside it a small count of the reports that wait to be sent (nothing
  // when none wait); it opens their list.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { openReport, outboxIndicator, report } from "./report.svelte";

  const waiting = $derived(outboxIndicator());
</script>

<span class="wrap">
  <button class="report-btn" aria-busy={report.busy} disabled={report.busy} onclick={openReport} title={t("report.button")} aria-label={t("report.button")}>
    <Icon name="bug" size={16} /><span class="label">{t("report.button")}</span>
  </button>
  {#if waiting.visible}
    <button class="count" onclick={() => (report.listOpen = true)} title={waiting.label} aria-label={waiting.label}>{waiting.count}</button>
  {/if}
</span>

<style>
  .wrap {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .report-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 99px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 12.5px;
    white-space: nowrap;
    cursor: pointer;
  }
  .report-btn:disabled {
    opacity: 0.6;
    cursor: progress;
  }
  .report-btn:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }
  /* A neutral number, not a badge: no colour fill. */
  .count {
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 99px;
    background: var(--surface);
    color: var(--text-2);
    font: 12px/1 ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    cursor: pointer;
  }
  .count:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  @media (max-width: 1100px) {
    .label {
      display: none;
    }
    .report-btn {
      padding: 0 9px;
    }
  }
</style>
