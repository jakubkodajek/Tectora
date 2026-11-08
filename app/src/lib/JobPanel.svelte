<script lang="ts">
  import { info, notifyError } from "./notify.svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import { api } from "./api";
  import { t } from "./i18n.svelte";
  import type { Attachment, Project } from "./types";
  import { JOB_STATUSES } from "./types";
  import { DACAD_MATERIALS, defaultMaterialName } from "./materials";

  /** Opens an attached file; files that are not drawings or documents are only shown in their folder. */
  async function openFile(name: string) {
    try {
      if (!(await api.openAttachment(id, name))) info(t("files.shownInFolder", { name }));
    } catch (e) {
      notifyError(e, "files");
    }
  }

  let {
    id,
    project,
    onchange,
    onsave,
  }: {
    id: string;
    project: Project;
    onchange: () => void;
    /** Writes pending edits to disk (before an export); false when that failed. */
    onsave: () => Promise<boolean>;
  } = $props();

  // Leaving the draft status stores the result the offer is made with.
  let status = $state(untrack(() => project.job.status));
  async function statusChanged() {
    const was = status;
    status = project.job.status;
    if (was === "draft" && status !== "draft" && !project.result_snapshot) {
      try {
        project.result_snapshot = await api.takeSnapshot(JSON.parse(JSON.stringify(project)));
      } catch (e) {
        notifyError(e, "save");
      }
    }
    onchange();
  }

  // Files attached to the job (drawings, photos).
  let files = $state<Attachment[]>([]);
  async function loadFiles() {
    try {
      files = await api.listAttachments(id);
    } catch (e) {
      notifyError(e, "files");
    }
  }
  $effect(() => {
    void id;
    untrack(loadFiles);
  });

  async function addFiles() {
    const sel = await open({ multiple: true });
    const paths = Array.isArray(sel) ? sel : sel ? [sel] : [];
    if (!paths.length) return;
    try {
      await api.attachFiles(id, paths);
    } catch (e) {
      notifyError(e, "files");
    }
    loadFiles();
  }

  async function removeFile(name: string) {
    // Tauri's dialog plugin makes window.confirm asynchronous (a Promise is
    // always truthy), so the answer must be awaited.
    if (!(await confirm(t("files.confirmRemove", { name })))) return;
    try {
      await api.removeAttachment(id, name);
    } catch (e) {
      notifyError(e, "files");
    }
    loadFiles();
  }

  async function exportJob() {
    const path = await save({
      defaultPath: `${project.name || "job"}.tectorajob`,
      filters: [{ name: "Tectora", extensions: ["tectorajob"] }],
    });
    if (!path) return;
    try {
      // A failed save keeps the edits only in memory: the bundle would hold
      // the older file (ERR-003). The save has shown its own error.
      if (!(await onsave())) {
        notifyError({ code: "not_saved", id }, "export");
        return;
      }
      await api.exportJob(id, path);
    } catch (e) {
      notifyError(e, "export");
    }
  }

  const size = (b: number) => (b < 1024 ? `${b} B` : b < 1048576 ? `${(b / 1024).toFixed(0)} kB` : `${(b / 1048576).toFixed(1)} MB`);

  const ids = Array.from({ length: DACAD_MATERIALS }, (_, i) => i + 1);

  function setName(m: number, e: Event) {
    const names = [...(project.material_names ?? [])];
    while (names.length < m) names.push("");
    names[m - 1] = (e.target as HTMLInputElement).value.trim();
    while (names.length && !names[names.length - 1]) names.pop();
    project.material_names = names;
    onchange();
  }
</script>

<div class="job">
  <label><span>{t("job.name")}</span><input type="text" bind:value={project.name} onchange={onchange} /></label>
  <div class="two">
    <label><span>{t("job.number")}</span><input type="text" bind:value={project.job.number} onchange={onchange} /></label>
    <label>
      <span>{t("job.status")}</span>
      <select bind:value={project.job.status} onchange={statusChanged}>
        {#each JOB_STATUSES as s}<option value={s}>{t(`status.${s}`)}</option>{/each}
      </select>
    </label>
  </div>
  <label><span>{t("job.customer")}</span><input type="text" bind:value={project.job.customer} onchange={onchange} /></label>
  <label><span>{t("job.address")}</span><input type="text" bind:value={project.job.address} onchange={onchange} /></label>
  <label><span>{t("job.notes")}</span><textarea rows="5" bind:value={project.notes} onchange={onchange}></textarea></label>
  <h4>{t("files.title")}</h4>
  {#if files.length}
    <ul class="files">
      {#each files as f (f.name)}
        <li>
          <button class="link" onclick={() => openFile(f.name)} title={t("files.open")}>{f.name}</button>
          <span class="size">{size(f.size)}</span>
          <button class="icon" onclick={() => removeFile(f.name)} title={t("files.remove")} aria-label={t("files.remove")}><Icon name="trash" size={14} /></button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="hint">{t("files.empty")}</p>
  {/if}
  <div class="row">
    <button class="btn sm" onclick={addFiles}><Icon name="plus" size={14} />{t("files.add")}</button>
    <button class="btn sm" onclick={exportJob} title={t("files.exportHint")}><Icon name="download" size={14} />{t("files.export")}</button>
  </div>
  <h4 title={t("job.materialsHint")}>{t("job.materials")}</h4>
  {#each ids as m}
    <label class="mat">
      <span>{m}</span>
      <input type="text" value={project.material_names?.[m - 1] ?? ""} placeholder={defaultMaterialName(m)}
        onchange={(e) => setName(m, e)} />
    </label>
  {/each}
</div>

<style>
  .job {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  input,
  select,
  textarea {
    width: 100%;
  }
  h4 {
    margin: 6px 0 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .files {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12.5px;
  }
  .files li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .link {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    border: 0;
    padding: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .size {
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .icon {
    border: 0;
    padding: 2px;
    background: none;
    color: var(--text-3);
    cursor: pointer;
  }
  .icon:hover {
    color: var(--danger);
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .row .btn {
    white-space: nowrap;
  }
  .mat {
    display: grid;
    grid-template-columns: 18px 1fr;
    align-items: center;
    gap: 6px;
  }
  .mat span {
    margin: 0;
    text-align: right;
  }
</style>
