<script lang="ts">
  // A settings change an AI client made (MCP update_settings, audit UX-003):
  // it applies at once, so the card says who changed what and why, with the
  // values before and after, and offers Revert (the previous values back)
  // and Accept. Folder changes are not here: they wait in FolderRequest.
  import { api } from "./api";
  import { actorName } from "./live.svelte";
  import { info, notifyError } from "./notify.svelte";
  import { t } from "./i18n.svelte";
  import { changeNote, changeRows, keptNote, rowLabel } from "./settingschange";
  import type { AiSettingsChange, AppSettings, AppSettingsView } from "./types";

  let {
    change,
    settings = null,
    onanswered,
  }: {
    change: AiSettingsChange;
    /** The settings now: a recorded value changed again since shows the current one too (UX-104). */
    settings?: AppSettings | null;
    onanswered: (view: AppSettingsView) => void;
  } = $props();

  let busy = $state(false);
  /** At small window heights the card shows its title row only, until opened (UI-105). */
  let open = $state(false);
  const rows = $derived(changeRows(change, settings));

  /** The card's height: in narrow windows the toasts stack above it, never over its answers (UX-201). */
  let cardHeight = $state(0);
  $effect(() => {
    const root = document.documentElement;
    root.style.setProperty("--ai-card-space", `${Math.ceil(cardHeight) + 8}px`);
    return () => root.style.removeProperty("--ai-card-space");
  });

  async function answer(revert: boolean) {
    // The pressed button stays enabled and focused while this waits
    // (aria-disabled): a disabled one drops the keyboard focus to the page (UI-604).
    if (busy) return;
    busy = true;
    // The card goes once the answer is in: `change` is gone with it.
    const id = change.id;
    try {
      const view = await api.answerAiChange(id, revert);
      // Values changed again since stay as they are: say so, not "reverted" silently (UX-104).
      const done = view.settings.ai_changes?.find((c) => c.id === id);
      onanswered(view);
      const kept = revert && done ? keptNote(done, view.settings) : null;
      if (kept) info(kept);
    } catch (e) {
      notifyError(e, "settings");
    } finally {
      busy = false;
    }
  }
</script>

<section class="card" class:open bind:clientHeight={cardHeight} role="status" aria-live="polite" aria-label={t("aiChange.title", { by: actorName(change.by) })}>
  <div class="head">
    <span class="dot"></span><strong>{t("aiChange.title", { by: actorName(change.by) })}</strong>
    <button class="more" aria-expanded={open} onclick={() => (open = !open)}>{t("review.details")}</button>
  </div>
  <div class="details">
    {#if change.summary}<p class="why">{change.summary}</p>{/if}
    <dl>
      {#each rows as r (r.key)}
        <dt>{rowLabel(r)}</dt>
        <dd><code>{r.before}</code> → <code>{r.after}</code>{#if r.now}{" "}<span class="now" data-testid="aichange-now">({t("aiChange.now", { v: r.now })})</span>{/if}</dd>
      {/each}
    </dl>
    <p class="muted">{changeNote(change)}</p>
  </div>
  <div class="foot">
    <button class="btn sm" aria-disabled={busy} aria-busy={busy} onclick={() => answer(true)}>{t("ai.revert")}</button>
    <button class="btn sm accept" aria-disabled={busy} aria-busy={busy} onclick={() => answer(false)}>{t("review.accept")}</button>
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
  /* Accept as in the review banner: orange means AI (UI-105). */
  .btn.accept {
    background: var(--ai);
    border-color: var(--ai);
    color: var(--on-ai);
  }
  .now {
    color: var(--text-2);
    font-size: 12px;
  }
  .more {
    display: none;
    margin-left: auto;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 12px;
    cursor: pointer;
  }
  /* At the minimum window size the card is its title row and the answers;
     the values open with Details, so the plan and the list stay reachable. */
  @media (max-height: 560px) {
    .more {
      display: inline-block;
    }
    .card:not(.open) .details {
      display: none;
    }
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
</style>
