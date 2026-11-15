<script lang="ts">
  // An AI client asked to read a file outside the jobs folder and the
  // drawings inbox (MCP drawing tools, attach_file, import_job; review
  // ARCH-002). It reads nothing until the user allows that one file here.
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { notifyError } from "./notify.svelte";
  import { t } from "./i18n.svelte";
  import { fileRequestPurpose } from "./filerequest";
  import type { FileRequest } from "./types";

  let { request, onanswered }: { request: FileRequest; onanswered: () => void } = $props();

  let busy = $state(false);

  async function answer(allow: boolean) {
    if (busy) return;
    busy = true;
    try {
      await api.answerFileRequest(allow);
      onanswered();
    } catch (e) {
      notifyError(e, "save");
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t("fileReq.title")} width={520} onclose={() => answer(false)}>
  <p>{t(fileRequestPurpose(request.purpose), { by: request.by })}</p>
  <p class="path"><code>{request.path}</code></p>
  <p class="muted">{t("fileReq.note")}</p>
  {#snippet footer()}
    <button class="btn" disabled={busy} onclick={() => answer(false)}>{t("fileReq.deny")}</button>
    <button class="btn primary" disabled={busy} onclick={() => answer(true)}>{t("folderReq.allow")}</button>
  {/snippet}
</Modal>

<style>
  .path {
    margin: 12px 0;
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--text-2);
    font-size: 12.5px;
  }
</style>
