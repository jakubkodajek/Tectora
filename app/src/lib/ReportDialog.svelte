<script lang="ts">
  // Report a bug: what happened in a few words, what the report contains.
  // The report is one file (crates/tectora-core/src/bugreport.rs). Send
  // queues it and sends it to the developers (or keeps it until it can be
  // sent); Save as writes it to a file the user chooses. A build that cannot
  // send offers Save as alone.
  import { save } from "@tauri-apps/plugin-dialog";
  import { onMount, untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { buildRequest, cleanReporterName, dialogLayout, loadReporterName, MAX_REPORTER_NAME, reportFailure, reportStamp, reportToast, saveReporterName, type Include } from "./bugreport";
  import { describeError } from "./errors";
  import { fileSize } from "./format";
  import { t } from "./i18n.svelte";
  import { notify, notifyError } from "./notify.svelte";
  import { closeReport, report, type Capture } from "./report.svelte";
  import { refreshOutbox } from "./reportqueue";
  import { ui } from "./theme.svelte";
  import { i18n } from "./i18n.svelte";

  let { capture }: { capture: Capture } = $props();

  // The dialog is mounted anew for each capture.
  const hasJob = untrack(() => !!capture.jobId);
  // Closing the dialog keeps what was typed (report.draft) until the report is sent or saved.
  let include = $state<Include>({ job: hasJob, customer: true, files: hasJob, history: hasJob, snapshot: hasJob, log: true });
  let files = $state<{ count: number; size: number } | null>(null);
  let busy = $state(false);
  /** Why the last try failed, when the reason needs a line of its own. */
  let hint = $state<string | null>(null);
  let reporterName = $state(loadStoredName());

  const layout = $derived(dialogLayout(report.sendable === true));

  function loadStoredName(): string {
    try {
      return loadReporterName(window.localStorage);
    } catch {
      return "";
    }
  }

  /** Escape, the X and the backdrop do nothing while the report is made: it would be made anyway. */
  const close = () => {
    if (!busy) closeReport();
  };

  onMount(() => {
    // Whether this build can send is read again: the dialog must not guess.
    refreshOutbox();
    if (!capture.jobId) return;
    api.listAttachments(capture.jobId).then(
      (list) => {
        files = { count: list.length, size: list.reduce((n, a) => n + a.size, 0) };
        if (!list.length) include.files = false;
      },
      () => (files = { count: 0, size: 0 }),
    );
  });

  async function send(saveAs: boolean) {
    // The pressed button stays enabled while this waits (aria-disabled).
    if (busy) return;
    busy = true;
    hint = null;
    try {
      let path: string | null = null;
      if (saveAs) {
        const name = await api.bugReportName(capture.jobName, reportStamp());
        const chosen = await save({ defaultPath: name, filters: [{ name: "ZIP", extensions: ["zip"] }] });
        if (typeof chosen !== "string") return;
        path = chosen;
      }
      const request = buildRequest({
        description: report.draft,
        jobId: capture.jobId,
        include: $state.snapshot(include) as Include,
        planSvg: capture.planSvg,
        modelPng: capture.modelPng,
        editorProject: capture.editorProject,
        ui: {
          locale: i18n.locale,
          theme: ui.theme,
          screen: capture.screen,
          layout: capture.layout,
          window: [window.innerWidth, window.innerHeight],
          webgl: capture.webgl,
        },
        leftOut: capture.leftOut,
        reporterName,
      });
      const result = await api.createBugReport(request, path);
      try {
        saveReporterName(window.localStorage, reporterName);
      } catch {
        // No storage: the name is not remembered.
      }
      const plan = reportToast(result);
      if (plan.clearDraft) report.draft = "";
      if (plan.closeDialog) closeReport();
      const showPath = plan.showPath;
      if (showPath) {
        // A saved file: the toast stays until closed, and can show it in its folder.
        notify({
          kind: plan.kind,
          title: plan.title,
          message: plan.message,
          sticky: true,
          action: { label: t("report.show"), run: () => api.showBugReport(showPath).catch((e) => notifyError(e, "reportShow")) },
        });
      } else {
        notify({ kind: plan.kind, title: plan.title, message: plan.message });
      }
      refreshOutbox();
    } catch (e) {
      // The dialog stays open with the text kept.
      const f = reportFailure(describeError(e, saveAs ? "reportSave" : "report").code);
      hint = f.hint;
      notifyError(e, saveAs ? "reportSave" : "report");
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t("report.title")} width={560} onclose={close}>
  <label class="what">
    <span>{t("report.what")}</span>
    <textarea rows="5" maxlength="5000" placeholder={t("report.whatHint")} disabled={busy} bind:value={report.draft}></textarea>
  </label>

  <fieldset>
    <legend>{t("report.included")}</legend>
    {#if hasJob}
      <label class="check"><input type="checkbox" bind:checked={include.job} disabled={busy} />{t("report.inc.job")}</label>
      <label class="check sub"><input type="checkbox" bind:checked={include.customer} disabled={busy || !include.job} />{t("report.inc.customer")}</label>
      {#if include.job}<small class="hint">{t("report.inc.customerHint")}</small>{/if}
      {#if files && files.count > 0}
        <label class="check sub"><input type="checkbox" bind:checked={include.files} disabled={busy || !include.job} />{t("report.inc.files", { count: files.count, size: fileSize(files.size) })}</label>
      {/if}
      <label class="check sub"><input type="checkbox" bind:checked={include.history} disabled={busy || !include.job} />{t("report.inc.history")}</label>
    {:else}
      <p class="hint">{t("report.noJob")}</p>
      <p class="hint">{t("report.libraryHint")}</p>
    {/if}
    {#if hasJob}
      <label class="check"><input type="checkbox" bind:checked={include.snapshot} disabled={busy} />{t("report.inc.snapshot")}</label>
    {/if}
    <label class="check"><input type="checkbox" bind:checked={include.log} disabled={busy} />{t("report.inc.log")}</label>
  </fieldset>

  {#if layout.primary === "send"}
    <label class="name">
      <span>{t("report.yourName")}</span>
      <input type="text" maxlength={MAX_REPORTER_NAME} disabled={busy} bind:value={reporterName} onblur={() => (reporterName = cleanReporterName(reporterName))} />
    </label>
  {/if}

  <p class="dest">{t(layout.infoKey)}</p>
  {#if hint}<p class="dest" role="alert">{hint}</p>{/if}

  {#snippet footer()}
    <button class="btn" disabled={busy} onclick={close}>{t("common.cancel")}</button>
    {#if layout.primary === "send"}
      <button class="btn" aria-disabled={busy} onclick={() => send(true)}>{t("report.saveAs")}</button>
      <button class="btn primary" aria-disabled={busy} aria-busy={busy} onclick={() => send(false)}>{busy ? t("report.sending") : t("report.send")}</button>
    {:else}
      <button class="btn primary" aria-disabled={busy} aria-busy={busy} onclick={() => send(true)}>{t("report.saveAs")}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .what {
    display: grid;
    gap: 6px;
    margin-bottom: 14px;
  }
  textarea {
    width: 100%;
    resize: vertical;
    box-sizing: border-box;
  }
  fieldset {
    margin: 0 0 12px;
    padding: 0;
    border: 0;
    display: grid;
    gap: 6px;
  }
  legend {
    padding: 0;
    margin-bottom: 6px;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .check input {
    width: auto;
  }
  .sub {
    margin-left: 22px;
  }
  .hint {
    margin: 0 0 0 22px;
    color: var(--text-2);
    font-size: 12px;
  }
  fieldset > p.hint {
    margin-left: 0;
  }
  .name {
    display: grid;
    gap: 6px;
    margin-bottom: 12px;
  }
  .dest {
    margin: 0 0 4px;
    color: var(--text-2);
    font-size: 12.5px;
    overflow-wrap: anywhere;
  }
</style>
