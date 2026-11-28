<script lang="ts">
  // Report a bug: what happened in a few words, what the report contains,
  // where it goes. The report is one file (crates/tectora-core/src/bugreport.rs)
  // saved into the reports folder of the Settings, or to a place chosen with
  // Save as. Nothing is sent anywhere.
  import { save } from "@tauri-apps/plugin-dialog";
  import { onMount, untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { buildRequest, leftOutText, reportStamp, type Include } from "./bugreport";
  import { describeError } from "./errors";
  import { fileSize } from "./format";
  import { t } from "./i18n.svelte";
  import { notify, notifyError } from "./notify.svelte";
  import { closeReport, report, type Capture } from "./report.svelte";
  import { app } from "./settings.svelte";
  import { ui } from "./theme.svelte";
  import { i18n } from "./i18n.svelte";

  let { capture }: { capture: Capture } = $props();

  // The dialog is mounted anew for each capture.
  const hasJob = untrack(() => !!capture.jobId);
  // Closing the dialog keeps what was typed (report.draft) until the report is saved.
  let include = $state<Include>({ job: hasJob, customer: true, files: hasJob, history: hasJob, snapshot: hasJob, log: true });
  let files = $state<{ count: number; size: number } | null>(null);
  let busy = $state(false);

  const folder = $derived(app.view?.effective.reports_dir ?? null);

  /** The last part of a path, for the toast ("Hlášení", not the whole path). */
  const baseName = (p: string) => p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || p;
  /** Codes of a reports folder that is gone, not a folder or not writable. */
  const FOLDER_CODES = ["file_not_found", "permission_denied", "invalid_input", "not_a_folder", "not_found"];

  /** Escape, the X and the backdrop do nothing while the report is written: it would be written anyway. */
  const close = () => {
    if (!busy) closeReport();
  };

  onMount(() => {
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
      });
      const saved = await api.createBugReport(request, path);
      report.draft = "";
      closeReport();
      const where = path ? t("report.passOn") : t("report.savedTo", { folder: baseName(folder ?? saved.path.replace(/[\\/][^\\/]*$/, "")) });
      notify({
        kind: "info",
        title: t("report.done"),
        message: [where, saved.file_name, saved.left_out.length ? t("report.leftOut", { what: saved.left_out.map(leftOutText).join(", ") }) : ""].filter(Boolean).join("\n"),
        // The user may have looked away: the toast stays until closed.
        sticky: true,
        action: { label: t("report.show"), run: () => api.showBugReport(saved.path).catch((e) => notifyError(e, "reportShow")) },
      });
    } catch (e) {
      // The dialog stays open with the text kept.
      const d = describeError(e, "report");
      if (folder && !saveAs && FOLDER_CODES.includes(d.code)) {
        // The reports folder is gone or read-only: say which one and that
        // Save as is the way on.
        notify({ kind: "error", code: d.code, title: d.title, message: t("report.destMissing"), detail: d.detail });
      } else {
        notifyError(e, "report");
      }
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

  <p class="dest">
    {#if folder}
      <span>{t("report.dest")}</span> <code title={folder}>{folder}</code>
    {:else}
      {t("report.destNone")} {t("report.destSettings")}
    {/if}
  </p>

  {#snippet footer()}
    <button class="btn" disabled={busy} onclick={close}>{t("common.cancel")}</button>
    {#if folder}
      <button class="btn" aria-disabled={busy} onclick={() => send(true)}>{t("report.saveAs")}</button>
      <button class="btn primary" aria-disabled={busy} aria-busy={busy} onclick={() => send(false)}>{busy ? t("report.sending") : t("report.save")}</button>
    {:else}
      <button class="btn primary" aria-disabled={busy} aria-busy={busy} onclick={() => send(true)}>{busy ? t("report.sending") : t("report.saveAs")}</button>
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
  .dest {
    margin: 0;
    color: var(--text-2);
    font-size: 12.5px;
    overflow-wrap: anywhere;
  }
</style>
