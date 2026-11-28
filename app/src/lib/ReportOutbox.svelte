<script lang="ts">
  // The reports that wait to be sent: when they were made, which job, what
  // keeps them (a code in monospace, with a text when it is a known one).
  // Send now tries all of them, each row can be saved as a file or discarded
  // (after asking). Opened from the count beside the report button.
  import { save } from "@tauri-apps/plugin-dialog";
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { outboxRowText, reportToast, type OutboxEntry } from "./bugreport";
  import { i18n, t } from "./i18n.svelte";
  import { notify, notifyError } from "./notify.svelte";
  import { report } from "./report.svelte";
  import { refreshOutbox } from "./reportqueue";

  let busy = $state(false);
  const rows = $derived(report.queue.map((e) => ({ e, text: outboxRowText(e) })));
  const canSend = $derived(report.sendable === true && report.outbox.waiting + report.outbox.failed > 0);

  // The list closes by itself when nothing is left.
  $effect(() => {
    if (report.listOpen && report.queue.length === 0 && report.outbox.waiting + report.outbox.failed + report.outbox.local === 0) report.listOpen = false;
  });

  const close = () => {
    if (!busy) report.listOpen = false;
  };

  const when = (ms: number) => (ms > 0 ? new Date(ms).toLocaleString(i18n.locale) : "");

  async function sendNow() {
    if (busy) return;
    busy = true;
    try {
      await api.sendBugReports();
    } catch (e) {
      notifyError(e, "report");
    } finally {
      busy = false;
      await refreshOutbox();
    }
  }

  async function saveAs(e: OutboxEntry) {
    if (busy) return;
    busy = true;
    try {
      const chosen = await save({ defaultPath: e.file_name, filters: [{ name: "ZIP", extensions: ["zip"] }] });
      if (typeof chosen !== "string") return;
      const saved = await api.saveQueuedBugReport(e.file_name, chosen);
      const plan = reportToast(saved);
      notify({ kind: plan.kind, title: plan.title, message: plan.message, sticky: true });
    } catch (err) {
      notifyError(err, "reportSave");
    } finally {
      busy = false;
    }
  }

  async function discard(e: OutboxEntry) {
    if (busy) return;
    // Under Tauri's dialog plugin confirm is async: always awaited.
    if (!(await window.confirm(t("report.outbox.discardAsk")))) return;
    busy = true;
    try {
      await api.discardBugReport(e.file_name);
    } catch (err) {
      notifyError(err, "report");
    } finally {
      busy = false;
      await refreshOutbox();
    }
  }
</script>

<Modal title={t("report.outbox.title")} width={600} onclose={close}>
  <ul class="rows">
    {#each rows as { e, text } (e.file_name)}
      <li>
        <div class="main">
          <span class="job">{text.job}</span>
          <span class="when">{when(e.created_ms)}</span>
        </div>
        <div class="state">
          <span>{text.state}</span>
          {#if text.error}
            <code>{text.error.code}</code>{#if text.error.text}<span class="why">{text.error.text}</span>{/if}
          {/if}
        </div>
        <div class="actions">
          <button class="btn ghost" aria-disabled={busy} onclick={() => saveAs(e)}>{t("report.outbox.saveAs")}</button>
          <button class="btn ghost" aria-disabled={busy} onclick={() => discard(e)}>{t("report.outbox.discard")}</button>
        </div>
      </li>
    {/each}
  </ul>

  {#snippet footer()}
    {#if canSend}
      <button class="btn primary" aria-disabled={busy} aria-busy={busy} onclick={sendNow}>{busy ? t("report.sending") : t("report.outbox.sendNow")}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px 12px;
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .main {
    display: flex;
    gap: 10px;
    align-items: baseline;
    min-width: 0;
  }
  .job {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .when,
  .state {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .state {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: baseline;
  }
  code {
    font: 12px/1.4 ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  }
  .actions {
    grid-row: 1 / span 2;
    grid-column: 2;
    display: flex;
    gap: 4px;
    align-items: center;
  }
</style>
