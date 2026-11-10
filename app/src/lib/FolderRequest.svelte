<script lang="ts">
  // An AI client asked to move the jobs folder or the drawings inbox
  // (MCP update_settings, review ARCH-052). Nothing moves until the user
  // allows it here; Keep closes the request.
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { notifyError } from "./notify.svelte";
  import { t } from "./i18n.svelte";
  import type { FolderRequest } from "./types";

  let { request, onanswered }: { request: FolderRequest; onanswered: (moved: boolean) => void } = $props();

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
    if (busy) return;
    busy = true;
    try {
      await api.answerFolderRequest(allow);
      onanswered(allow);
    } catch (e) {
      notifyError(e, "save");
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t("folderReq.title")} width={520} onclose={() => answer(false)}>
  <p>{t("folderReq.lead", { by: request.by })}</p>
  <dl>
    {#each rows as r (r.label)}
      <dt>{r.label}</dt>
      <dd><code>{r.value}</code></dd>
    {/each}
  </dl>
  {#if request.summary}<p class="why">{request.summary}</p>{/if}
  <p class="muted">{t("folderReq.note")}</p>
  {#snippet footer()}
    <button class="btn" disabled={busy} onclick={() => answer(false)}>{t("folderReq.keep")}</button>
    <button class="btn primary" disabled={busy} onclick={() => answer(true)}>{t("folderReq.allow")}</button>
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
  }
  .muted {
    color: var(--text-2);
    font-size: 12.5px;
  }
</style>
