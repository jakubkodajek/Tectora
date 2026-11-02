<script lang="ts">
  import { t } from "./i18n.svelte";
  import type { Project } from "./types";
  import { JOB_STATUSES } from "./types";
  import { DACAD_MATERIALS, defaultMaterialName } from "./materials";

  let { project, onchange }: { project: Project; onchange: () => void } = $props();

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
      <select bind:value={project.job.status} onchange={onchange}>
        {#each JOB_STATUSES as s}<option value={s}>{t(`status.${s}`)}</option>{/each}
      </select>
    </label>
  </div>
  <label><span>{t("job.customer")}</span><input type="text" bind:value={project.job.customer} onchange={onchange} /></label>
  <label><span>{t("job.address")}</span><input type="text" bind:value={project.job.address} onchange={onchange} /></label>
  <label><span>{t("job.notes")}</span><textarea rows="5" bind:value={project.notes} onchange={onchange}></textarea></label>
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
