<script lang="ts">
  import { LOCALES, i18n, setLocale, t, type Locale } from "./i18n.svelte";
  import { updateSettings } from "./settings.svelte";

  function choose(l: Locale) {
    setLocale(l);
    // Also store it in the settings file; the choice applies even if that fails.
    updateSettings({ locale: l }).catch(() => {});
  }
</script>

<label class="lang" title={t("lang")}>
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
    <circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" />
  </svg>
  <select
    aria-label={t("lang")}
    value={i18n.locale}
    onchange={(e) => choose((e.target as HTMLSelectElement).value as Locale)}
  >
    {#each LOCALES as l}
      <option value={l.id}>{l.label}</option>
    {/each}
  </select>
</label>

<style>
  .lang {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 8px;
    border-radius: 8px;
    color: var(--text-2);
  }
  .lang:hover {
    background: var(--surface-2);
  }
  select {
    appearance: none;
    -webkit-appearance: none;
    padding: 0 2px;
    font: inherit;
    color: var(--text);
    background: transparent;
    border: none;
    outline: none;
    cursor: pointer;
  }
</style>
