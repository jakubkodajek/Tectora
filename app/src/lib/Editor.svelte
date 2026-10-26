<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import EdgePanel from "./EdgePanel.svelte";
  import Icon from "./Icon.svelte";
  import PlanView from "./PlanView.svelte";
  import Results from "./Results.svelte";
  import Roof3D from "./Roof3D.svelte";
  import { api, onProjectsChanged } from "./api";
  import LanguagePicker from "./LanguagePicker.svelte";
  import { t } from "./i18n.svelte";
  import type { CalcResult, EdgeDef, Project, Selection } from "./types";

  let {
    id,
    onback,
    onerror,
    onnotice,
  }: { id: string; onback: () => void; onerror: (msg: string) => void; onnotice: (msg: string) => void } = $props();

  let project = $state<Project | null>(null);
  let result = $state<CalcResult | null>(null);
  let selection = $state<Selection>(null);
  let view = $state<"split" | "plan" | "3d">("split");
  let saveState = $state<"saved" | "saving" | "dirty">("saved");
  let externalFlash = $state(false);

  // Undo history of serialized projects; `cursor` points at the current one.
  let history: string[] = [];
  let cursor = $state(0);
  let historyLen = $state(0);
  // Last JSON written by us, to tell our own saves from external edits.
  let lastSaved = "";
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let calcSeq = 0;

  const snapshot = () => JSON.stringify(project);

  async function recalc() {
    if (!project) return;
    const seq = ++calcSeq;
    try {
      const r = await api.calc($state.snapshot(project) as Project);
      if (seq === calcSeq) result = r;
    } catch (e) {
      onerror(String(e));
    }
  }

  function scheduleSave() {
    saveState = "dirty";
    clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 500);
  }

  async function save() {
    if (!project) return;
    clearTimeout(saveTimer);
    saveState = "saving";
    const json = snapshot();
    try {
      await api.saveProject(id, JSON.parse(json));
      lastSaved = JSON.stringify(JSON.parse(json));
      saveState = "saved";
    } catch (e) {
      saveState = "dirty";
      onerror(String(e));
    }
  }

  /** Records the current state as an undo step, recalculates and saves. */
  function commit() {
    const s = snapshot();
    if (history[cursor] === s) return;
    history = history.slice(0, cursor + 1);
    history.push(s);
    cursor = history.length - 1;
    historyLen = history.length;
    recalc();
    scheduleSave();
  }

  function restore(i: number) {
    cursor = i;
    project = JSON.parse(history[i]);
    selection = null;
    recalc();
    scheduleSave();
  }

  const undo = () => cursor > 0 && restore(cursor - 1);
  const redo = () => cursor < history.length - 1 && restore(cursor + 1);

  async function load(initial: boolean) {
    try {
      const p = await api.loadProject(id);
      const json = JSON.stringify(p);
      if (!initial && json === lastSaved) return;
      project = p;
      lastSaved = json;
      if (initial) {
        history = [snapshot()];
        cursor = 0;
        historyLen = 1;
      } else {
        // External change (CLI, MCP, AI agent): keep it undoable.
        history = history.slice(0, cursor + 1);
        history.push(snapshot());
        cursor = history.length - 1;
        historyLen = history.length;
        externalFlash = true;
        setTimeout(() => (externalFlash = false), 2500);
        onnotice(t("editor.externalNotice"));
      }
      recalc();
    } catch (e) {
      onerror(String(e));
    }
  }

  let unlisten: (() => void) | undefined;
  onMount(() => {
    load(true);
    onProjectsChanged((ids) => {
      if (ids.includes(id) && saveState !== "saving") load(false);
    }).then((u) => (unlisten = u));
  });
  onDestroy(() => {
    unlisten?.();
    if (saveState === "dirty") save();
  });

  function patchEdge(patch: Partial<EdgeDef>) {
    if (!project || !selection) return;
    Object.assign(project.parts[selection.part].edges[selection.edge], patch);
    commit();
  }

  function applyAll(patch: Partial<EdgeDef>) {
    if (!project || !selection) return;
    for (const e of project.parts[selection.part].edges) {
      if ("pitch_deg" in patch && e.pitch_deg >= 89.999) continue;
      Object.assign(e, patch);
    }
    commit();
  }

  /** Changes an edge length by moving its end vertex along the edge. */
  function setLength(len: number) {
    if (!project || !selection) return;
    const es = project.parts[selection.part].edges;
    const a = es[selection.edge].start;
    const b = es[(selection.edge + 1) % es.length].start;
    const cur = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    b.x = +(a.x + ((b.x - a.x) * len) / cur).toFixed(3);
    b.y = +(a.y + ((b.y - a.y) * len) / cur).toFixed(3);
    commit();
  }

  function deleteVertex() {
    if (!project || !selection) return;
    const es = project.parts[selection.part].edges;
    if (es.length <= 3) return;
    es.splice(selection.edge, 1);
    selection = null;
    commit();
  }

  function moveVertex(part: number, v: number, x: number, y: number) {
    if (!project) return;
    const p = project.parts[part].edges[v].start;
    if (p.x === x && p.y === y) return;
    p.x = x;
    p.y = y;
    recalc();
  }

  function insertVertex(part: number, edge: number, x: number, y: number) {
    if (!project) return;
    const es = project.parts[part].edges;
    const copy: EdgeDef = { ...$state.snapshot(es[edge]), start: { x, y }, half_hip: null, direction_rad: null };
    es.splice(edge + 1, 0, copy);
    selection = { part, edge: edge + 1 };
    commit();
  }

  function rename(e: Event) {
    if (!project) return;
    const v = (e.target as HTMLInputElement).value.trim();
    if (v && v !== project.name) {
      project.name = v;
      commit();
    }
  }

  function onkeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      e.shiftKey ? redo() : undo();
    } else if (mod && e.key.toLowerCase() === "y") {
      e.preventDefault();
      redo();
    } else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (e.key === "Escape") {
      selection = null;
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="editor">
  <header>
    <button class="icon-btn" onclick={onback} aria-label={t("editor.back")} title={t("editor.back")}><Icon name="back" /></button>
    {#if project}
      <input class="title" type="text" value={project.name} onchange={rename} aria-label={t("editor.name")} />
    {/if}
    <span class="status" class:flash={externalFlash}>
      {#if externalFlash}{t("editor.external")}{:else if saveState === "saved"}<Icon name="check" size={14} />{t("editor.saved")}{:else if saveState === "saving"}{t("editor.saving")}{:else}{t("editor.dirty")}{/if}
    </span>
    <div class="spacer"></div>
    <button class="icon-btn" onclick={undo} disabled={cursor <= 0} title="{t('editor.undo')} (Ctrl+Z)" aria-label={t("editor.undo")}><Icon name="undo" /></button>
    <button class="icon-btn" onclick={redo} disabled={cursor >= historyLen - 1} title="{t('editor.redo')} (Ctrl+Shift+Z)" aria-label={t("editor.redo")}><Icon name="redo" /></button>
    <LanguagePicker />
    <div class="seg" role="radiogroup" aria-label={t("editor.both")}>
      <button class:on={view === "plan"} onclick={() => (view = "plan")} title={t("editor.plan")}><Icon name="plan" size={16} />2D</button>
      <button class:on={view === "split"} onclick={() => (view = "split")} title={t("editor.both")}><Icon name="split" size={16} /></button>
      <button class:on={view === "3d"} onclick={() => (view = "3d")} title={t("editor.model")}><Icon name="cube" size={16} />3D</button>
    </div>
  </header>

  {#if project}
    <div class="body">
      <div class="views" class:split={view === "split"}>
        {#if view !== "3d"}
          <div class="view">
            <PlanView
              {project}
              {result}
              bind:selection
              onmove={moveVertex}
              onmoveend={commit}
              oninsert={insertVertex}
            />
          </div>
        {/if}
        {#if view !== "plan"}
          <div class="view"><Roof3D {project} {result} {selection} /></div>
        {/if}
      </div>
      <aside>
        <section>
          {#if selection && project.parts[selection.part]}
            <EdgePanel
              part={project.parts[selection.part]}
              index={selection.edge}
              onchange={patchEdge}
              onapplyall={applyAll}
              onlength={setLength}
              ondelete={deleteVertex}
            />
          {:else}
            <div class="tip">
              <h3>{t("editor.tipTitle")}</h3>
              <p class="muted">{t("editor.tip")}</p>
            </div>
          {/if}
        </section>
        <section>
          <h3>{t("editor.results")}</h3>
          <Results totals={result?.totals ?? null} />
        </section>
      </aside>
    </div>
  {/if}
</div>

<style>
  .editor {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .title {
    width: 300px;
    border-color: transparent;
    background: transparent;
    font-weight: 600;
    font-size: 15px;
  }
  .title:hover {
    border-color: var(--border);
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
    color: var(--text-3);
    padding: 3px 8px;
    border-radius: 99px;
    transition: background 0.3s, color 0.3s;
  }
  .status.flash {
    background: color-mix(in srgb, var(--focus) 15%, transparent);
    color: var(--focus);
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .seg {
    display: flex;
    margin-left: 8px;
    padding: 3px;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .seg button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
    font-weight: 500;
    font-size: 13px;
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .views {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: 1fr;
    gap: 1px;
    background: var(--border);
  }
  .views.split {
    grid-template-columns: 1fr 1fr;
  }
  .view {
    min-width: 0;
    min-height: 0;
  }
  aside {
    width: 320px;
    flex: none;
    overflow: auto;
    background: var(--surface);
    border-left: 1px solid var(--border);
  }
  aside section {
    padding: 18px;
    border-bottom: 1px solid var(--border);
  }
  aside h3 {
    margin: 0 0 12px;
    font-size: 15px;
  }
  .tip p {
    margin: 0;
    font-size: 13px;
  }
</style>
