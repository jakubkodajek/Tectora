<script lang="ts">
  // The top-bar button of the bug report: icon and label, the label hidden
  // at the compact breakpoint (the icon keeps the name for screen readers).
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { openReport } from "./report.svelte";
</script>

<button class="report-btn" onclick={openReport} title={t("report.button")} aria-label={t("report.button")}>
  <Icon name="bug" size={16} /><span class="label">{t("report.button")}</span>
</button>

<style>
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
  .report-btn:hover {
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
