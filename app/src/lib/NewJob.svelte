<script lang="ts">
  // New job: job data plus how to start the roof (standard roof, example,
  // a roof from a past job or drawing the footprint), like DACAD's "Neu" +
  // "Standard-Dach".
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import PlanThumb from "./PlanThumb.svelte";
  import StandardRoofForm from "./StandardRoofForm.svelte";
  import { api } from "./api";
  import { m2, relativeTime } from "./format";
  import { notifyError } from "./notify.svelte";
  import { t } from "./i18n.svelte";
  import type { ExampleInfo, ProjectSummary, RoofPart } from "./types";
  import { emptyProject } from "./types";

  let {
    examples,
    projects = [],
    initialExample = null,
    initialSource = null,
    onclose,
    oncreated,
  }: {
    examples: ExampleInfo[];
    /** The library, for starting from a past job's roof. */
    projects?: ProjectSummary[];
    initialExample?: string | null;
    /** Preselected past job (id) to copy the roof from. */
    initialSource?: string | null;
    onclose: () => void;
    oncreated: (id: string) => void;
  } = $props();

  let name = $state("");
  let number = $state("");
  let customer = $state("");
  let address = $state("");
  // Initial choices only; later prop changes must not reset the form.
  let start = $state<"standard" | "example" | "past" | "draw">(
    untrack(() => (initialSource ? "past" : initialExample ? "example" : "standard")),
  );
  let example = $state<string>(untrack(() => initialExample ?? examples[0]?.id ?? "gable"));
  let source = $state<string | null>(untrack(() => initialSource));
  let pastQuery = $state("");
  let withFiles = $state(false);
  const norm = (s: string) => s.toLocaleLowerCase("cs").normalize("NFD").replace(/\p{M}/gu, "");
  const past = $derived.by(() => {
    const words = norm(pastQuery).split(/\s+/).filter(Boolean);
    return projects.filter((p) => {
      const hay = norm([p.name, p.job.number, p.job.customer, p.job.address, p.notes].join("\n"));
      return words.every((w) => hay.includes(w));
    });
  });
  const sourceJob = $derived(projects.find((p) => p.id === source) ?? null);
  let part = $state<RoofPart | null>(null);
  let busy = $state(false);

  async function create() {
    busy = true;
    try {
      const title = name.trim() || (start === "example" ? t(`ex.${example}.name`) : t("home.newName"));
      if (start === "past") {
        if (!sourceJob) return;
        const job = { number, customer, address };
        oncreated(await api.duplicateProject(sourceJob.id, name.trim() || sourceJob.name, job, withFiles));
        return;
      }
      let project = emptyProject(title);
      if (start === "example") {
        const id = await api.createFromExample(example, title);
        project = await api.loadProject(id);
        project.job = { number, customer, address, status: "draft" };
        oncreated(await api.saveProject(id, project));
        return;
      }
      if (start === "standard" && part) project.parts = [{ ...$state.snapshot(part), name: t("editor.mainPart") }];
      project.job = { number, customer, address, status: "draft" };
      oncreated(await api.saveProject(null, project));
    } catch (e) {
      notifyError(e, "create");
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t("new.title")} width={720} {onclose}>
  <div class="job">
    <label class="wide"><span>{t("job.name")}</span><input type="text" bind:value={name} placeholder={t("new.namePlaceholder")} /></label>
    <label><span>{t("job.number")}</span><input type="text" bind:value={number} /></label>
    <label><span>{t("job.customer")}</span><input type="text" bind:value={customer} /></label>
    <label class="wide"><span>{t("job.address")}</span><input type="text" bind:value={address} /></label>
  </div>

  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={start === "standard"} class:on={start === "standard"} onclick={() => (start = "standard")}>{t("new.standard")}</button>
    <button role="tab" aria-selected={start === "example"} class:on={start === "example"} onclick={() => (start = "example")}>{t("new.example")}</button>
    <button role="tab" aria-selected={start === "past"} class:on={start === "past"} onclick={() => (start = "past")}>{t("new.past")}</button>
    <button role="tab" aria-selected={start === "draw"} class:on={start === "draw"} onclick={() => (start = "draw")}>{t("new.draw")}</button>
  </div>

  {#if start === "standard"}
    <StandardRoofForm bind:part />
  {:else if start === "example"}
    <div class="examples">
      {#each examples as ex (ex.id)}
        <button class="example" class:on={example === ex.id} onclick={() => (example = ex.id)}>
          <div class="thumb"><PlanThumb outlines={ex.outlines} lines={ex.lines} padding={0.12} /></div>
          <strong>{t(`ex.${ex.id}.name`)}</strong>
          <span>{t(`ex.${ex.id}.desc`)}</span>
        </button>
      {/each}
    </div>
  {:else if start === "past"}
    {#if projects.length === 0}
      <p class="draw">{t("new.pastEmpty")}</p>
    {:else}
      <input type="search" class="past-search" placeholder={t("new.pastSearch")} bind:value={pastQuery} />
      <div class="past" role="listbox" aria-label={t("new.past")}>
        {#each past as p (p.id)}
          <button role="option" aria-selected={source === p.id} class:on={source === p.id} onclick={() => (source = p.id)} ondblclick={create}>
            <div class="mini"><PlanThumb outlines={p.outlines} lines={p.lines} /></div>
            <span class="past-text">
              <strong>{p.name}</strong>
              <small>{[p.job.customer, p.job.address].filter(Boolean).join(" · ") || "—"}</small>
            </span>
            <span class="past-meta"><span>{m2(p.roof_area)}</span><small>{relativeTime(p.modified_ms)}</small></span>
          </button>
        {:else}
          <p class="none">{t("new.pastNoMatch")}</p>
        {/each}
      </div>
      <div class="past-foot">
        <span class="muted">{t("new.pastHint")}</span>
        {#if sourceJob?.files}
          <label class="check"><input type="checkbox" bind:checked={withFiles} />{t("new.pastFiles")} ({sourceJob.files})</label>
        {/if}
      </div>
    {/if}
  {:else}
    <p class="draw">{t("new.drawHint")}</p>
  {/if}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t("common.cancel")}</button>
    <button class="btn primary" disabled={busy || (start === "standard" && !part) || (start === "past" && !sourceJob)} onclick={create}>{t("new.create")}</button>
  {/snippet}
</Modal>

<style>
  .job {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 12px;
    margin-bottom: 16px;
  }
  .wide {
    grid-column: span 2;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  input {
    width: 100%;
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 3px;
    margin-bottom: 14px;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .tabs button {
    flex: 1;
    height: 32px;
    border: none;
    border-radius: 7px;
    background: transparent;
    font-weight: 500;
    color: var(--text-2);
  }
  .tabs button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  .examples {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .example {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px;
    text-align: left;
    border: 1.5px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }
  .example.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .example strong {
    font-size: 13px;
  }
  .example span {
    font-size: 11.5px;
    color: var(--text-2);
    line-height: 1.3;
  }
  .thumb {
    height: 64px;
    margin-bottom: 4px;
  }
  .past-search {
    margin-bottom: 8px;
  }
  .past {
    display: flex;
    flex-direction: column;
    gap: 2px;
    height: 264px;
    overflow-y: auto;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .past button {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
    padding: 5px 8px;
    border: 1.5px solid transparent;
    border-radius: 8px;
    background: none;
    text-align: left;
  }
  .past button:hover {
    background: var(--surface-2);
  }
  .past button.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .mini {
    flex: none;
    width: 44px;
    height: 34px;
  }
  .past-text,
  .past-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .past-text {
    flex: 1;
  }
  .past-text strong,
  .past-text small {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .past-meta {
    align-items: flex-end;
    font-variant-numeric: tabular-nums;
  }
  .past small {
    font-size: 11.5px;
    color: var(--text-2);
  }
  .none {
    margin: 0;
    padding: 18px;
    color: var(--text-2);
  }
  .past-foot {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 6px 16px;
    margin-top: 8px;
    font-size: 12px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .check input {
    width: auto;
  }
  .draw {
    margin: 0;
    padding: 18px;
    border-radius: 10px;
    background: var(--surface-2);
    color: var(--text-2);
  }
</style>
