<script lang="ts">
  import { t } from "./i18n.svelte";
  import type { Project } from "./types";
  import { JOB_STATUSES } from "./types";

  let { project, onchange }: { project: Project; onchange: () => void } = $props();
</script>

<div class="job">
  <label><span>{t("job.name")}</span><input type="text" bind:value={project.name} onchange={onchange} /></label>
  <div class="two">
    <label><span>{t("job.number")}</span><input type="text" bind:value={project.job.number} onchange={onchange} /></label>
    <label>
      <span>{t("job.status")}</span>
      <select bind:value={project.job.status} onchange={onchange}>
        {#each JOB_STATUSES as s}<option value={s}>{t(`status.${s}`)}</option>{/each}
      </select>
    </label>
  </div>
  <label><span>{t("job.customer")}</span><input type="text" bind:value={project.job.customer} onchange={onchange} /></label>
  <label><span>{t("job.address")}</span><input type="text" bind:value={project.job.address} onchange={onchange} /></label>
  <label><span>{t("job.notes")}</span><textarea rows="5" bind:value={project.notes} onchange={onchange}></textarea></label>
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
</style>
