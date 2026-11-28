<script lang="ts">
  // An AI client asked to move the jobs folder or the drawings inbox
  // (MCP update_settings, review ARCH-052). Nothing moves until the user
  // allows it here; Keep closes the request.
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { mayLeave } from "./leave";
  import { notifyError } from "./notify.svelte";
  import { t } from "./i18n.svelte";
  import { actorName } from "./live.svelte";
  import type { FolderRequest } from "./types";

  let { request, onanswered, jobOpen = false }: { request: FolderRequest; onanswered: (moved: boolean) => void; jobOpen?: boolean } = $props();

  let busy = $state(false);
  const rows = $derived(
    (["projects_dir", "drawings_dir"] as const)
      .filter((k) => k in request.changes)
      .map((k) => ({
        label: t(k === "projects_dir" ? "settings.projectsDir" : "settings.drawingsDir"),
        value: request.changes[k] || t("folderReq.default"),
      })),
  );

  async function answer(allow: boolean) {
    // The pressed button stays enabled and focused while this waits
    // (aria-disabled): a disabled one drops the keyboard focus to the page (UI-604).
    if (busy) return;
    busy = true;
    try {
      // Allowing a new jobs folder closes the open job: it is a way out of
      // the editor like Back or the Settings field, so the editor saves
      // first, waits for a Revert under way and asks about edits that cannot
      // be saved. Before the call: the core switches the folder in it. "No"
      // keeps the editor, the folder and this request (HUNT-1201).
      if (allow && "projects_dir" in request.changes && !(await mayLeave())) return;
      await api.answerFolderRequest($state.snapshot(request.changes), allow);
      onanswered(allow);
    } catch (e) {
      notifyError(e, "settings");
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t("folderReq.title")} width={520} onclose={() => answer(false)}>
  <p>{t("folderReq.lead", { by: actorName(request.by) })}</p>
  <dl>
    {#each rows as r (r.label)}
      <dt>{r.label}</dt>
      <dd><code>{r.value}</code></dd>
    {/each}
  </dl>
  <!-- The AI's own words, quoted after a lead naming it: never read as Tectora's advice (UI-1604). -->
  {#if request.summary}<p class="why" data-testid="folder-reason">{t("folderReq.reason", { by: actorName(request.by) })} {t("ai.quoted", { s: request.summary })}</p>{/if}
  <!-- What Allow does to the jobs: the open one closes, the others stay behind (UX-1302). -->
  {#if "projects_dir" in request.changes}<p class="muted" data-testid="folder-consequence">{jobOpen ? `${t("folderReq.jobCloses")} ` : ""}{t("folderReq.jobsStay")}</p>{/if}
  <p class="muted">{t("folderReq.note")}</p>
  {#snippet footer()}
    <button class="btn" aria-disabled={busy} aria-busy={busy} onclick={() => answer(false)}>{t("folderReq.keep")}</button>
    <button class="btn primary" aria-disabled={busy} aria-busy={busy} onclick={() => answer(true)}>{t("folderReq.allow")}</button>
  {/snippet}
</Modal>

<style>
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 12px 0;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .why {
    color: var(--text-2);
    font-style: italic;
    /* An unbroken reason (a path, a URL) wraps inside the dialog (HUNT-1707). */
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--text-2);
    font-size: 12.5px;
  }
</style>
