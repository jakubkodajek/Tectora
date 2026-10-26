<script lang="ts">
  // New job: job data plus how to start the roof (standard roof, example
  // or drawing the footprint), like DACAD's "Neu" + "Standard-Dach".
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import PlanThumb from "./PlanThumb.svelte";
  import StandardRoofForm from "./StandardRoofForm.svelte";
  import { api } from "./api";
  import { t } from "./i18n.svelte";
  import type { ExampleInfo, RoofPart } from "./types";
  import { emptyProject } from "./types";

  let {
    examples,
    initialExample = null,
    onclose,
    oncreated,
    onerror,
  }: {
    examples: ExampleInfo[];
    initialExample?: string | null;
    onclose: () => void;
    oncreated: (id: string) => void;
    onerror: (msg: string) => void;
  } = $props();

  let name = $state("");
  let number = $state("");
  let customer = $state("");
  let address = $state("");
  // Initial choices only; later prop changes must not reset the form.
  let start = $state<"standard" | "example" | "draw">(untrack(() => (initialExample ? "example" : "standard")));
  let example = $state<string>(untrack(() => initialExample ?? examples[0]?.id ?? "gable"));
  let part = $state<RoofPart | null>(null);
  let busy = $state(false);

  async function create() {
    busy = true;
    try {
      const title = name.trim() || (start === "example" ? t(`ex.${example}.name`) : t("home.newName"));
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
      onerror(String(e));
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
  {:else}
    <p class="draw">{t("new.drawHint")}</p>
  {/if}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t("common.cancel")}</button>
    <button class="btn primary" disabled={busy || (start === "standard" && !part)} onclick={create}>{t("new.create")}</button>
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
  .draw {
    margin: 0;
    padding: 18px;
    border-radius: 10px;
    background: var(--surface-2);
    color: var(--text-2);
  }
</style>
