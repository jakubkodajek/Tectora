<script lang="ts">
  // Job library (design manager): all roofs the user has made, with job
  // data, status filters, sorting, a detail pane and the example gallery.
  import { open } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import LanguagePicker from "./LanguagePicker.svelte";
  import Logo from "./Logo.svelte";
  import NewJob from "./NewJob.svelte";
  import PlanThumb from "./PlanThumb.svelte";
  import ProjectCard from "./ProjectCard.svelte";
  import { api } from "./api";
  import { m2, relativeTime } from "./format";
  import { t } from "./i18n.svelte";
  import { actorName, isFresh, live } from "./live.svelte";
  import { prefs, savePrefs } from "./settings.svelte";
  import type { ExampleInfo, JobStatus, ProjectSummary } from "./types";
  import { JOB_STATUSES, isAi } from "./types";

  let {
    projects,
    examples,
    dir,
    onopen,
    onchanged,
    onerror,
    onconnect,
  }: {
    projects: ProjectSummary[];
    examples: ExampleInfo[];
    dir: string;
    onopen: (id: string) => void;
    onchanged: () => void;
    onerror: (msg: string) => void;
    onconnect: () => void;
  } = $props();

  type Filter = "all" | JobStatus | "ai" | "examples";
  type SortKey = "modified" | "name" | "number" | "customer" | "area";
  let query = $state("");
  let filter = $state<Filter>("all");
  let sort = $state<{ key: SortKey; desc: boolean }>({ key: "modified", desc: true });
  let selectedId = $state<string | null>(null);
  let newJob = $state<{ example: string | null } | null>(null);

  const norm = (s: string) => s.toLocaleLowerCase("cs").normalize("NFD").replace(/\p{M}/gu, "");
  const counts = $derived.by(() => {
    const c: Record<string, number> = { all: projects.length, ai: 0 };
    for (const s of JOB_STATUSES) c[s] = 0;
    for (const p of projects) {
      c[p.job.status] = (c[p.job.status] ?? 0) + 1;
      if (p.last_activity && isAi(p.last_activity.actor)) c.ai++;
    }
    return c;
  });

  const rows = $derived.by(() => {
    const q = norm(query.trim());
    const list = projects.filter((p) => {
      if (filter === "ai" && !(p.last_activity && isAi(p.last_activity.actor))) return false;
      if (filter !== "all" && filter !== "ai" && filter !== "examples" && p.job.status !== filter) return false;
      if (!q) return true;
      return [p.name, p.job.number, p.job.customer, p.job.address, p.notes].some((s) => norm(s).includes(q));
    });
    const key = (p: ProjectSummary): string | number =>
      sort.key === "modified" ? p.modified_ms
      : sort.key === "area" ? p.roof_area
      : sort.key === "name" ? norm(p.name)
      : sort.key === "number" ? norm(p.job.number)
      : norm(p.job.customer);
    return [...list].sort((a, b) => {
      const x = key(a);
      const y = key(b);
      const r = x < y ? -1 : x > y ? 1 : 0;
      return sort.desc ? -r : r;
    });
  });
  const selected = $derived(projects.find((p) => p.id === selectedId) ?? null);

  function sortBy(key: SortKey) {
    sort = sort.key === key ? { key, desc: !sort.desc } : { key, desc: key === "modified" || key === "area" };
  }

  async function run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    try {
      return await fn();
    } catch (e) {
      onerror(String(e));
    }
  }

  /** Adds a job from a .tectorajob bundle or a .tectora file. */
  async function importJob() {
    const path = await open({ multiple: false, filters: [{ name: "Tectora", extensions: ["tectorajob", "tectora"] }] });
    if (typeof path !== "string") return;
    const id = await run(() => api.importJob(path));
    if (id) {
      onchanged();
      onopen(id);
    }
  }

  async function remove(p: ProjectSummary) {
    if (!confirm(t("home.confirmDelete", { name: p.name }))) return;
    await run(() => api.deleteProject(p.id));
    if (selectedId === p.id) selectedId = null;
    onchanged();
  }

  async function duplicate(p: ProjectSummary) {
    const id = await run(() => api.duplicateProject(p.id, t("lib.copyName", { name: p.name })));
    if (id) selectedId = id;
    onchanged();
  }

  function setView(v: "list" | "grid") {
    prefs.libraryView = v;
    savePrefs();
  }

  const FILTERS: { id: Filter; icon: string }[] = [
    { id: "all", icon: "folder" },
    { id: "draft", icon: "draw" },
    { id: "offer", icon: "briefcase" },
    { id: "ordered", icon: "check" },
    { id: "done", icon: "roof" },
  ];
</script>

<div class="page">
  <header>
    <div class="brand"><Logo size={30} /><span>Tectora</span></div>
    <label class="search">
      <Icon name="search" size={16} />
      <input type="search" placeholder={t("lib.search")} bind:value={query} />
    </label>
    <div class="actions">
      <button class="ai-btn" class:active={isFresh()} onclick={onconnect}>
        <span class="dot"></span>
        {#if isFresh() && live.status}{actorName(live.status.actor)}: {live.status.message}{:else}{t("ai.connect")}{/if}
      </button>
      <LanguagePicker />
      <button class="btn" onclick={importJob} title={t("home.importJobHint")}><Icon name="upload" size={16} />{t("home.importJob")}</button>
      <button class="btn primary" onclick={() => (newJob = { example: null })}><Icon name="plus" size={16} />{t("lib.new")}</button>
    </div>
  </header>

  <div class="layout">
    <nav>
      <h3>{t("lib.title")}</h3>
      {#each FILTERS as f}
        <button class:on={filter === f.id} onclick={() => (filter = f.id)}>
          <Icon name={f.icon} size={16} />{f.id === "all" ? t("lib.all") : t(`status.${f.id}`)}<span>{counts[f.id] ?? 0}</span>
        </button>
      {/each}
      <button class:on={filter === "ai"} onclick={() => (filter = "ai")}>
        <Icon name="sparkle" size={16} />{t("lib.byAi")}<span>{counts.ai}</span>
      </button>
      <hr />
      <button class:on={filter === "examples"} onclick={() => (filter = "examples")}>
        <Icon name="house" size={16} />{t("home.examples")}<span>{examples.length}</span>
      </button>
      <div class="folder" title={dir}><Icon name="folder" size={13} /><span>{`\u200E${dir}\u200E`}</span></div>
    </nav>

    <main>
      {#if filter === "examples"}
        <div class="head"><h2>{t("home.examples")}</h2><p class="muted">{t("home.examplesHint")}</p></div>
        <div class="examples">
          {#each examples as ex (ex.id)}
            <button class="example" onclick={() => (newJob = { example: ex.id })}>
              <div class="ex-thumb"><PlanThumb outlines={ex.outlines} lines={ex.lines} padding={0.12} /></div>
              <div class="ex-text"><strong>{t(`ex.${ex.id}.name`)}</strong><span class="muted">{t(`ex.${ex.id}.desc`)}</span></div>
            </button>
          {/each}
        </div>
      {:else}
        <div class="head">
          <h2>{filter === "all" ? t("lib.all") : filter === "ai" ? t("lib.byAi") : t(`status.${filter}`)} <span class="count">{rows.length}</span></h2>
          <div class="seg">
            <button class:on={prefs.libraryView === "list"} onclick={() => setView("list")} aria-label={t("lib.list")} title={t("lib.list")}><Icon name="list" size={16} /></button>
            <button class:on={prefs.libraryView === "grid"} onclick={() => setView("grid")} aria-label={t("lib.grid")} title={t("lib.grid")}><Icon name="grid" size={16} /></button>
          </div>
        </div>

        {#if rows.length === 0}
          <div class="empty">
            {#if projects.length === 0}
              <Icon name="roof" size={36} />
              <p>{t("home.empty")}</p>
              <button class="btn primary" onclick={() => (newJob = { example: null })}><Icon name="plus" size={16} />{t("lib.new")}</button>
            {:else}
              <p>{t("home.noMatch", { q: query })}</p>
            {/if}
          </div>
        {:else if prefs.libraryView === "grid"}
          <div class="grid">
            {#each rows as p (p.id)}
              <ProjectCard project={p} onopen={() => onopen(p.id)} onduplicate={() => duplicate(p)} ondelete={() => remove(p)} />
            {/each}
          </div>
        {:else}
          <table>
            <thead>
              <tr>
                <th class="th-thumb"></th>
                {#each [["name", "lib.colName"], ["number", "job.number"], ["customer", "job.customer"]] as [k, label]}
                  <th><button onclick={() => sortBy(k as SortKey)}>{t(label)}{#if sort.key === k}{sort.desc ? " ↓" : " ↑"}{/if}</button></th>
                {/each}
                <th>{t("job.status")}</th>
                <th class="num"><button onclick={() => sortBy("area")}>{t("lib.colArea")}{#if sort.key === "area"}{sort.desc ? " ↓" : " ↑"}{/if}</button></th>
                <th><button onclick={() => sortBy("modified")}>{t("lib.colModified")}{#if sort.key === "modified"}{sort.desc ? " ↓" : " ↑"}{/if}</button></th>
              </tr>
            </thead>
            <tbody>
              {#each rows as p (p.id)}
                <tr class:sel={selectedId === p.id} onclick={() => (selectedId = p.id)} ondblclick={() => onopen(p.id)}>
                  <td class="th-thumb"><div class="mini"><PlanThumb outlines={p.outlines} lines={p.lines} /></div></td>
                  <td><strong>{p.name}</strong>{#if p.job.address}<small>{p.job.address}</small>{/if}</td>
                  <td>{p.job.number || "—"}</td>
                  <td>{p.job.customer || "—"}</td>
                  <td><span class="chip {p.job.status}">{t(`status.${p.job.status}`)}</span></td>
                  <td class="num">{m2(p.roof_area)}</td>
                  <td class="mod">
                    {relativeTime(p.modified_ms)}
                    {#if p.last_activity && isAi(p.last_activity.actor)}
                      <span class="ai-tag" title={p.last_activity.summary}><Icon name="sparkle" size={11} />{actorName(p.last_activity.actor)}</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/if}
    </main>

    {#if selected && filter !== "examples" && prefs.libraryView === "list"}
      <aside class="detail">
        <div class="big"><PlanThumb outlines={selected.outlines} lines={selected.lines} /></div>
        <h3>{selected.name}</h3>
        <span class="chip {selected.job.status}">{t(`status.${selected.job.status}`)}</span>
        <dl>
          <dt>{t("job.number")}</dt><dd>{selected.job.number || "—"}</dd>
          <dt>{t("job.customer")}</dt><dd>{selected.job.customer || "—"}</dd>
          <dt>{t("job.address")}</dt><dd>{selected.job.address || "—"}</dd>
          <dt>{t("res.area")}</dt><dd>{m2(selected.roof_area)}</dd>
          <dt>{t("lib.colModified")}</dt><dd>{relativeTime(selected.modified_ms)}</dd>
        </dl>
        {#if selected.notes}<p class="notes">{selected.notes}</p>{/if}
        {#if selected.last_activity}
          <div class="last" class:ai={isAi(selected.last_activity.actor)}>
            <span>{isAi(selected.last_activity.actor) ? actorName(selected.last_activity.actor) : t("ai.you")} · {relativeTime(selected.last_activity.ts_ms)}</span>
            {selected.last_activity.summary}
          </div>
        {/if}
        <div class="detail-actions">
          <button class="btn primary" onclick={() => onopen(selected!.id)}><Icon name="open" size={16} />{t("card.open")}</button>
          <button class="btn" onclick={() => duplicate(selected!)}><Icon name="copy" size={16} />{t("card.duplicate")}</button>
          <button class="btn ghost danger" onclick={() => remove(selected!)}><Icon name="trash" size={16} />{t("card.delete")}</button>
        </div>
      </aside>
    {/if}
  </div>
</div>

{#if newJob}
  <NewJob
    {examples}
    initialExample={newJob.example}
    onclose={() => (newJob = null)}
    oncreated={(id) => {
      newJob = null;
      onopen(id);
    }}
    {onerror}
  />
{/if}

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
    padding: 12px 20px;
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
    align-items: center;
    gap: 10px;
  }
  .ai-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    max-width: 320px;
    height: 34px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 99px;
    background: var(--surface);
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ai-btn .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .ai-btn.active {
    background: var(--ai-soft);
    border-color: color-mix(in srgb, var(--ai) 50%, var(--border));
  }
  .ai-btn.active .dot {
    background: var(--ai);
  }
  .layout {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  nav {
    width: 220px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 10px;
    background: var(--surface);
    border-right: 1px solid var(--border);
  }
  nav h3 {
    margin: 0 8px 8px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border: none;
    border-radius: 8px;
    background: none;
    text-align: left;
    color: var(--text-2);
    font-weight: 500;
  }
  nav button span {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-3);
  }
  nav button:hover {
    background: var(--surface-2);
  }
  nav button.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  nav hr {
    width: 100%;
    margin: 8px 0;
    border: none;
    border-top: 1px solid var(--border);
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: auto;
    padding: 0 8px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .folder span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    direction: rtl;
    text-align: left;
  }
  main {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: 20px 24px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 14px;
  }
  .head p {
    margin: 0;
    font-size: 13px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 650;
  }
  .count {
    margin-left: 6px;
    padding: 1px 8px;
    border-radius: 99px;
    background: var(--surface-2);
    color: var(--text-2);
    font-size: 12px;
    font-weight: 600;
    vertical-align: middle;
  }
  .seg {
    display: flex;
    margin-left: auto;
    padding: 3px;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .seg button {
    display: grid;
    place-items: center;
    width: 32px;
    height: 28px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    font-size: 13px;
  }
  th {
    position: sticky;
    top: 0;
    padding: 0 10px;
    height: 36px;
    text-align: left;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  th button {
    padding: 0;
    border: none;
    background: none;
    font: inherit;
    color: inherit;
  }
  td {
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  tbody tr:last-child td {
    border-bottom: none;
  }
  tbody tr {
    cursor: default;
  }
  tbody tr:hover {
    background: color-mix(in srgb, var(--surface-2) 60%, transparent);
  }
  tbody tr.sel {
    background: var(--accent-soft);
  }
  td small {
    display: block;
    color: var(--text-3);
    font-size: 12px;
  }
  .th-thumb {
    width: 58px;
  }
  .mini {
    width: 42px;
    height: 34px;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .mod {
    white-space: nowrap;
    color: var(--text-2);
  }
  .ai-tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 99px;
    background: var(--ai-soft);
    color: var(--ai-strong);
    font-size: 11px;
    font-weight: 600;
  }
  .chip {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 99px;
    font-size: 11.5px;
    font-weight: 600;
    white-space: nowrap;
    background: var(--surface-2);
    color: var(--text-2);
  }
  .chip.offer {
    background: #fdf1dc;
    color: #8a5a10;
  }
  .chip.ordered {
    background: #e2edfb;
    color: #24539b;
  }
  .chip.done {
    background: #e1f3e8;
    color: #22703f;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 18px;
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
  }
  .ex-text span {
    font-size: 12.5px;
  }
  .empty {
    display: grid;
    place-items: center;
    gap: 10px;
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
  .detail {
    width: 300px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 18px;
    overflow: auto;
    background: var(--surface);
    border-left: 1px solid var(--border);
  }
  .big {
    width: 100%;
    height: 180px;
    padding: 14px;
    border-radius: 10px;
    background:
      linear-gradient(var(--plan-grid) 1px, transparent 1px) 0 0 / 16px 16px,
      linear-gradient(90deg, var(--plan-grid) 1px, transparent 1px) 0 0 / 16px 16px,
      var(--surface);
    border: 1px solid var(--border);
  }
  .detail h3 {
    margin: 4px 0 0;
    font-size: 16px;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 12px;
    margin: 4px 0 0;
    width: 100%;
    font-size: 13px;
  }
  dt {
    color: var(--text-3);
  }
  dd {
    margin: 0;
  }
  .notes {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
    white-space: pre-wrap;
  }
  .last {
    width: 100%;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--surface-2);
    font-size: 12.5px;
  }
  .last.ai {
    background: var(--ai-soft);
  }
  .last span {
    display: block;
    font-weight: 600;
    color: var(--text-2);
  }
  .detail-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    width: 100%;
  }
  .danger {
    color: var(--danger);
  }
</style>
