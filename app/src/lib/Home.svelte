<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import PlanThumb from "./PlanThumb.svelte";
  import ProjectCard from "./ProjectCard.svelte";
  import { api } from "./api";
  import type { ExampleInfo, ProjectSummary } from "./types";

  let {
    projects,
    examples,
    dir,
    onopen,
    onchanged,
    onerror,
  }: {
    projects: ProjectSummary[];
    examples: ExampleInfo[];
    dir: string;
    onopen: (id: string) => void;
    onchanged: () => void;
    onerror: (msg: string) => void;
  } = $props();

  let query = $state("");
  const filtered = $derived(
    projects.filter((p) => p.name.toLocaleLowerCase("cs").includes(query.trim().toLocaleLowerCase("cs"))),
  );

  async function run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    try {
      return await fn();
    } catch (e) {
      onerror(String(e));
    }
  }

  async function fromExample(id: string) {
    const newId = await run(() => api.createFromExample(id));
    if (newId) onopen(newId);
  }

  async function importDad() {
    const path = await open({
      multiple: false,
      filters: [{ name: "DACAD", extensions: ["dad", "DAD"] }],
    });
    if (typeof path !== "string") return;
    const id = await run(() => api.importDad(path));
    if (id) onopen(id);
  }

  async function remove(p: ProjectSummary) {
    if (!confirm(`Smazat návrh „${p.name}“? Tuto akci nelze vrátit.`)) return;
    await run(() => api.deleteProject(p.id));
    onchanged();
  }

  async function duplicate(p: ProjectSummary) {
    await run(() => api.duplicateProject(p.id));
    onchanged();
  }
</script>

<div class="page">
  <header>
    <div class="brand">
      <svg viewBox="0 0 1024 1024" width="30" height="30" aria-hidden="true">
        <rect x="64" y="64" width="896" height="896" rx="200" fill="#1f2a44" />
        <polygon points="512,250 200,560 400,640 512,420" fill="#e0662e" />
        <polygon points="512,250 824,560 624,640 512,420" fill="#a83c18" />
        <polygon points="400,640 512,420 624,640 512,720" fill="#e9eef7" />
      </svg>
      <span>Tectora</span>
    </div>
    <label class="search">
      <Icon name="search" size={16} />
      <input type="search" placeholder="Hledat návrhy" bind:value={query} />
    </label>
    <div class="actions">
      <button class="btn" onclick={importDad}><Icon name="upload" size={16} />Importovat z DACADu</button>
      <button class="btn primary" onclick={() => fromExample("gable")}><Icon name="plus" size={16} />Nový návrh</button>
    </div>
  </header>

  <main>
    <section>
      <div class="section-head">
        <h2>Začít z příkladu</h2>
        <p class="muted">Vyberte typ střechy a upravte ho podle svého půdorysu.</p>
      </div>
      <div class="examples">
        {#each examples as ex (ex.id)}
          <button class="example" onclick={() => fromExample(ex.id)}>
            <div class="ex-thumb"><PlanThumb outlines={ex.outlines} padding={0.12} /></div>
            <div class="ex-text">
              <strong>{ex.name}</strong>
              <span class="muted">{ex.description}</span>
            </div>
          </button>
        {/each}
      </div>
    </section>

    <section>
      <div class="section-head">
        <h2>Moje návrhy <span class="count">{projects.length}</span></h2>
        <p class="muted folder" title={dir}><Icon name="folder" size={14} />{dir}</p>
      </div>
      {#if filtered.length === 0}
        <div class="empty">
          {#if projects.length === 0}
            <Icon name="roof" size={36} />
            <p>Zatím tu nejsou žádné návrhy. Začněte příkladem nahoře nebo importujte soubor z DACADu.</p>
          {:else}
            <p>Hledání „{query}“ nic nenašlo.</p>
          {/if}
        </div>
      {:else}
        <div class="grid">
          {#each filtered as p (p.id)}
            <ProjectCard
              project={p}
              onopen={() => onopen(p.id)}
              onduplicate={() => duplicate(p)}
              ondelete={() => remove(p)}
            />
          {/each}
        </div>
      {/if}
    </section>
  </main>
</div>

<style>
  .page {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 14px 28px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 700;
    font-size: 17px;
    letter-spacing: -0.01em;
  }
  .search {
    flex: 1;
    max-width: 420px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 36px;
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--text-3);
  }
  .search input {
    flex: 1;
    border: none;
    background: transparent;
    height: 34px;
    padding: 0;
    box-shadow: none;
  }
  .actions {
    margin-left: auto;
    display: flex;
    gap: 10px;
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 34px;
  }
  .section-head {
    display: flex;
    align-items: baseline;
    gap: 14px;
    margin-bottom: 14px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .section-head p {
    margin: 0;
    font-size: 13px;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto !important;
    max-width: 50%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .count {
    display: inline-block;
    margin-left: 6px;
    padding: 1px 8px;
    border-radius: 99px;
    background: var(--surface-2);
    color: var(--text-2);
    font-size: 12px;
    font-weight: 600;
    vertical-align: middle;
  }
  .examples {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 12px;
  }
  .example {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px;
    text-align: left;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .example:hover {
    border-color: var(--accent);
    box-shadow: var(--shadow);
  }
  .ex-thumb {
    flex: none;
    width: 64px;
    height: 64px;
    padding: 6px;
    border-radius: 8px;
    background: var(--surface-2);
  }
  .ex-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .ex-text strong {
    font-size: 14px;
    font-weight: 600;
  }
  .ex-text span {
    font-size: 12.5px;
    line-height: 1.35;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 18px;
  }
  .empty {
    display: grid;
    place-items: center;
    gap: 8px;
    padding: 56px 20px;
    border: 1.5px dashed var(--border);
    border-radius: var(--radius);
    color: var(--text-3);
    text-align: center;
  }
  .empty p {
    margin: 0;
    max-width: 420px;
    color: var(--text-2);
  }
</style>
