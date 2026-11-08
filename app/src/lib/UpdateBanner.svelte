<script lang="ts">
  // Offers the newer version of the app the update server has. Shown only in
  // the job library, where nothing unsaved can be lost: installing ends the
  // app and starts the new version. Without internet it offers nothing (the
  // backend logs why).
  import { onMount } from "svelte";
  import { api } from "./api";
  import { t } from "./i18n.svelte";
  import { notifyError } from "./notify.svelte";
  import type { AvailableUpdate, UpdateProgress } from "./types";
  import { installQuestion, installStatus } from "./update";

  let update = $state<AvailableUpdate | null>(null);
  // The download's progress while installing (null before its first report); undefined otherwise.
  let progress = $state<UpdateProgress | null | undefined>(undefined);

  onMount(() => {
    api.updateCheck().then(
      (found) => (update = found),
      () => {},
    );
  });

  async function install() {
    if (!update || progress !== undefined) return;
    if (!(await confirm(installQuestion(t, update)))) return;
    progress = null;
    try {
      // On success the app ends and starts again in the new version.
      await api.updateInstall((p) => (progress = p));
    } catch (e) {
      notifyError(e, "update");
    } finally {
      progress = undefined;
    }
  }
</script>

{#if update}
  <div class="update" role="status">
    <i class="dot"></i>
    {#if progress !== undefined}
      <span class="text">{installStatus(t, update.version, progress)}</span>
    {:else}
      <span class="text">{t("update.available", { version: update.version })}</span>
      <button class="btn sm" onclick={install}>{t("update.install")}</button>
    {/if}
  </div>
{/if}

<style>
  .update {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 14px;
    padding: 8px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    font-size: 13px;
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 1px;
    background: var(--accent);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
</style>
