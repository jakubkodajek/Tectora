<script lang="ts">
  // A settings change an AI client made (MCP update_settings, audit UX-003):
  // it applies at once, so the card says who changed what and why, with the
  // values before and after, and offers Revert (the previous values back)
  // and Accept. Folder changes are not here: they wait in FolderRequest.
  import { api } from "./api";
  import { actorName } from "./live.svelte";
  import { notifyError } from "./notify.svelte";
  import { t } from "./i18n.svelte";
  import { changeRows } from "./settingschange";
  import type { AiSettingsChange, AppSettingsView } from "./types";

  let { change, onanswered }: { change: AiSettingsChange; onanswered: (view: AppSettingsView) => void } = $props();

  let busy = $state(false);
  const rows = $derived(changeRows(change));

  async function answer(revert: boolean) {
    if (busy) return;
    busy = true;
    try {
      onanswered(await api.answerAiChange(change.id, revert));
    } catch (e) {
      notifyError(e, "settings");
    } finally {
      busy = false;
    }
  }
</script>

<section class="card" role="status" aria-live="polite" aria-label={t("aiChange.title", { by: actorName(change.by) })}>
  <div class="head"><span class="dot"></span><strong>{t("aiChange.title", { by: actorName(change.by) })}</strong></div>
  {#if change.summary}<p class="why">{change.summary}</p>{/if}
  <dl>
    {#each rows as r (r.key)}
      <dt>{r.label}</dt>
      <dd><code>{r.before}</code> → <code>{r.after}</code></dd>
    {/each}
  </dl>
  <p class="muted">{t("aiChange.note")}</p>
  <div class="foot">
    <button class="btn sm" disabled={busy} onclick={() => answer(true)}>{t("ai.revert")}</button>
    <button class="btn sm primary" disabled={busy} onclick={() => answer(false)}>{t("review.accept")}</button>
  </div>
</section>

<style>
  .card {
    position: fixed;
    left: 16px;
    bottom: 40px;
    z-index: 100;
    width: min(380px, calc(100vw - 32px));
    padding: 10px 12px;
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
    font-size: 13px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 1px;
    background: var(--ai);
  }
  strong {
    font-weight: 600;
  }
  .why {
    margin: 4px 0 0 14px;
    color: var(--text-2);
    font-style: italic;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 12px;
    margin: 8px 0 0 14px;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
  }
  code {
    font: 12px/1.4 ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  }
  .muted {
    margin: 8px 0 0 14px;
    color: var(--text-3);
    font-size: 12px;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
</style>
