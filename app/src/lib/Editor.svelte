<script lang="ts">
  // Design editor laid out like DACAD (toolbar, plan + 3D windows, roof
  // definition bar, status bar) with a side dock for results, AI activity
  // and job data.
  import { onDestroy, onMount, untrack } from "svelte";
  import AiPanel from "./AiPanel.svelte";
  import DrawBar from "./DrawBar.svelte";
  import Icon from "./Icon.svelte";
  import JobPanel from "./JobPanel.svelte";
  import LanguagePicker from "./LanguagePicker.svelte";
  import Modal from "./Modal.svelte";
  import PlanView from "./PlanView.svelte";
  import Results from "./Results.svelte";
  import RoofDefBar from "./RoofDefBar.svelte";
  import Roof3D from "./Roof3D.svelte";
  import StandardRoofForm from "./StandardRoofForm.svelte";
  import { api, onProjectsChanged } from "./api";
  import { changedEdges, describeChange } from "./changes";
  import { m2, num } from "./format";
  import { t } from "./i18n.svelte";
  import { actorName, isFresh, live } from "./live.svelte";
  import { markReviewed, prefs, presets, reviewedAt, savePrefs } from "./settings.svelte";
  import type { ActivityItem, CalcResult, Change, EdgeDef, Point2, Project, RoofPart, Selection } from "./types";
  import { isAi } from "./types";

  let {
    id,
    onback,
    onerror,
    onconnect,
  }: { id: string; onback: () => void; onerror: (msg: string) => void; onconnect: () => void } = $props();

  let project = $state<Project | null>(null);
  let result = $state<CalcResult | null>(null);
  let selection = $state<Selection>(null);
  let saveState = $state<"saved" | "saving" | "dirty">("saved");
  let dock = $state<"results" | "ai" | "job">("results");
  let drawing = $state<Point2[] | null>(null);
  let cursor = $state<Point2 | null>(null);
  let showDims = $state(true);
  let activity = $state<ActivityItem[]>([]);
  let accepted = $state(untrack(() => reviewedAt(id)));
  let focusChange = $state<Change | null>(null);
  let std = $state<{ part: RoofPart | null; mode: "add" | "replace" } | null>(null);
  let plan: PlanView | undefined = $state();

  // Undo history of serialized projects; `cursor` points at the current one.
  let history: string[] = [];
  let at = $state(0);
  let historyLen = $state(0);
  // Last JSON written by us, to tell our own saves from external edits.
  let lastSaved = "";
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let calcSeq = 0;

  const snapshot = () => JSON.stringify(project);

  /** The newest AI change the user has not accepted yet. */
  const pending = $derived(activity[0] && isAi(activity[0].actor) && activity[0].ts_ms > accepted ? activity[0] : null);
  const highlight = $derived.by(() => {
    if (!project) return new Set<string>();
    if (focusChange) return changedEdges([focusChange], project);
    return pending ? changedEdges(pending.changes, project) : new Set<string>();
  });
  const wallCount = $derived(project?.parts.reduce((n, p) => n + p.edges.length, 0) ?? 0);

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

  async function refreshActivity() {
    try {
      activity = await api.projectActivity(id);
    } catch {
      activity = [];
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

  function pushHistory() {
    const s = snapshot();
    if (history[at] === s) return false;
    history = history.slice(0, at + 1);
    history.push(s);
    at = history.length - 1;
    historyLen = history.length;
    return true;
  }

  /** Records the current state as an undo step, recalculates and saves. */
  function commit() {
    if (!pushHistory()) return;
    recalc();
    scheduleSave();
  }

  function restore(i: number) {
    at = i;
    project = JSON.parse(history[i]);
    selection = null;
    recalc();
    scheduleSave();
  }

  const undo = () => at > 0 && restore(at - 1);
  const redo = () => at < history.length - 1 && restore(at + 1);

  async function load(initial: boolean) {
    try {
      const p = await api.loadProject(id);
      const json = JSON.stringify(p);
      await refreshActivity();
      if (!initial && json === lastSaved) return;
      project = p;
      lastSaved = json;
      if (initial) {
        history = [snapshot()];
        at = 0;
        historyLen = 1;
        if (p.parts.length === 0) drawing = [];
      } else {
        // External change (AI agent, CLI): keep it undoable and show it.
        pushHistory();
        if (pending) dock = "ai";
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

  function accept() {
    if (!pending) return;
    accepted = pending.ts_ms;
    markReviewed(id, pending.ts_ms);
  }

  async function revert() {
    try {
      clearTimeout(saveTimer);
      const p = await api.revertLastChange(id);
      lastSaved = JSON.stringify(p);
      project = p;
      saveState = "saved";
      pushHistory();
      selection = null;
      recalc();
      await refreshActivity();
      accepted = activity[0]?.ts_ms ?? accepted;
      markReviewed(id, accepted);
    } catch (e) {
      onerror(String(e));
    }
  }

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

  function deletePart() {
    if (!project || !selection) return;
    if (!confirm(t("editor.confirmDeletePart", { name: project.parts[selection.part].name }))) return;
    project.parts.splice(selection.part, 1);
    selection = null;
    commit();
  }

  // Drawing a new footprint.
  function startDraw() {
    selection = null;
    drawing = [];
  }

  function drawStep(dx: number, dy: number) {
    if (!drawing || drawing.length === 0) return;
    const last = drawing[drawing.length - 1];
    drawing.push({ x: +(last.x + dx).toFixed(3), y: +(last.y + dy).toFixed(3) });
  }

  function finishDraw() {
    if (!project || !drawing || drawing.length < 3) return;
    let pts = drawing;
    const area = pts.reduce((s, p, i) => {
      const q = pts[(i + 1) % pts.length];
      return s + p.x * q.y - q.x * p.y;
    }, 0);
    if (area < 0) pts = [...pts].reverse();
    const n = project.parts.length;
    project.parts.push({
      name: n === 0 ? t("editor.mainPart") : t("editor.partN", { n: n + 1 }),
      edges: pts.map((p) => ({ start: { ...p }, pitch_deg: presets.pitch_deg, eave_height: presets.eave_height, overhang: presets.overhang })),
    });
    drawing = null;
    selection = { part: n, edge: 0 };
    commit();
    setTimeout(() => plan?.fit(), 0);
  }

  function applyStandard() {
    if (!project || !std?.part) return;
    const part = $state.snapshot(std.part) as RoofPart;
    if (std.mode === "replace" && project.parts.length) {
      part.name = project.parts[selection?.part ?? 0].name;
      project.parts[selection?.part ?? 0] = part;
    } else {
      // Place a new part to the right of the existing ones.
      const xs = project.parts.flatMap((p) => p.edges.map((e) => e.start.x));
      const dx = xs.length ? Math.max(...xs) + 2 : 0;
      for (const e of part.edges) e.start.x = +(e.start.x + dx).toFixed(3);
      part.name = project.parts.length === 0 ? t("editor.mainPart") : t("editor.partN", { n: project.parts.length + 1 });
      project.parts.push(part);
    }
    std = null;
    drawing = null;
    selection = null;
    commit();
    setTimeout(() => plan?.fit(), 0);
  }

  function setLayout(l: "split" | "plan" | "3d") {
    prefs.layout = l;
    savePrefs();
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
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
    if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      e.shiftKey ? redo() : undo();
    } else if (mod && e.key.toLowerCase() === "y") {
      e.preventDefault();
      redo();
    } else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (e.key === "F3") {
      e.preventDefault();
      setLayout(prefs.layout === "3d" ? "split" : "3d");
    } else if (e.key === "Insert") {
      startDraw();
    } else if (e.key === "Escape" && !drawing) {
      selection = null;
    } else if (e.key === "Delete" && selection && !drawing) {
      deleteVertex();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="editor">
  <header>
    <button class="icon-btn" onclick={onback} title={t("editor.back")} aria-label={t("editor.back")}><Icon name="back" /></button>
    <div class="crumbs">
      <button class="crumb" onclick={onback}>{t("lib.title")}</button>
      <span>/</span>
      {#if project}
        <input class="title" type="text" value={project.name} onchange={rename} aria-label={t("editor.name")} />
        {#if project.job.number}<span class="jobno">{project.job.number}</span>{/if}
      {/if}
    </div>
    <div class="spacer"></div>
    <button class="ai-pill" class:active={isFresh()} onclick={() => (dock = "ai")} title={t("ai.title")}>
      <span class="dot"></span>
      {#if isFresh() && live.status}
        <strong>{actorName(live.status.actor)}</strong><span class="msg">{live.status.message}</span>
      {:else}
        <Icon name="sparkle" size={14} />{t("ai.short")}
      {/if}
    </button>
    <LanguagePicker />
  </header>

  <div class="toolbar" role="toolbar">
    <div class="group">
      <button class="tool" onclick={undo} disabled={at <= 0} title="{t('editor.undo')} (Ctrl+Z)"><Icon name="undo" /><span>{t("editor.undo")}</span></button>
      <button class="tool" onclick={redo} disabled={at >= historyLen - 1} title="{t('editor.redo')} (Ctrl+Shift+Z)"><Icon name="redo" /><span>{t("editor.redo")}</span></button>
    </div>
    <div class="group">
      <button class="tool" class:on={!!std} onclick={() => (std = { part: null, mode: project?.parts.length ? "replace" : "add" })} title={t("tool.standardHint")}><Icon name="house" /><span>{t("tool.standard")}</span></button>
      <button class="tool" class:on={!!drawing} onclick={startDraw} title="{t('tool.drawHint')} (Insert)"><Icon name="draw" /><span>{t("tool.draw")}</span></button>
      <button class="tool" disabled={!selection || (project?.parts.length ?? 0) < 2} onclick={deletePart} title={t("tool.deletePartHint")}><Icon name="trash" /><span>{t("tool.deletePart")}</span></button>
    </div>
    <div class="group">
      <button class="tool" class:on={prefs.layout === "plan"} onclick={() => setLayout("plan")}><Icon name="plan" /><span>{t("editor.plan")}</span></button>
      <button class="tool" class:on={prefs.layout === "split"} onclick={() => setLayout("split")}><Icon name="split" /><span>{t("editor.both")}</span></button>
      <button class="tool" class:on={prefs.layout === "3d"} onclick={() => setLayout("3d")} title="F3"><Icon name="cube" /><span>{t("editor.model")}</span></button>
      <button class="tool" onclick={() => plan?.fit()} disabled={prefs.layout === "3d"}><Icon name="fit" /><span>{t("plan.fit")}</span></button>
      <button class="tool" class:on={showDims} onclick={() => (showDims = !showDims)}><Icon name="eye" /><span>{t("tool.dims")}</span></button>
    </div>
  </div>

  {#if project}
    <div class="body">
      <div class="main">
        {#if pending}
          <div class="review" role="status">
            <Icon name="sparkle" size={18} />
            <div class="review-text">
              <strong>{t("review.title", { who: actorName(pending.actor) })}</strong>
              <span>{pending.summary}</span>
              {#if pending.changes.length}
                <span class="review-list">
                  {pending.changes.slice(0, 3).map((c) => describeChange(c, project!.parts.length > 1)).join(" · ")}{pending.changes.length > 3 ? ` · ${t("review.more", { n: pending.changes.length - 3 })}` : ""}
                </span>
              {/if}
            </div>
            <button class="btn sm" onclick={revert}><Icon name="undo" size={14} />{t("ai.revert")}</button>
            <button class="btn sm accept" onclick={accept}><Icon name="check" size={14} />{t("review.accept")}</button>
          </div>
        {/if}
        <div class="views" class:split={prefs.layout === "split"}>
          {#if prefs.layout !== "3d"}
            <div class="view">
              <span class="view-label">{t("editor.plan")}</span>
              <PlanView
                bind:this={plan}
                {project}
                {result}
                {highlight}
                {drawing}
                {showDims}
                bind:selection
                onmove={moveVertex}
                onmoveend={commit}
                oninsert={insertVertex}
                ondrawpoint={(x, y) => drawing?.push({ x, y })}
                ondrawclose={finishDraw}
                oncursor={(p) => (cursor = p)}
              />
            </div>
          {/if}
          {#if prefs.layout !== "plan"}
            <div class="view">
              <span class="view-label">{t("editor.model")}</span>
              <Roof3D {project} {result} {selection} />
            </div>
          {/if}
        </div>
        {#if drawing}
          <DrawBar
            count={drawing.length}
            onstep={drawStep}
            onundo={() => drawing?.pop()}
            onclose={finishDraw}
            oncancel={() => (drawing = null)}
          />
        {:else}
          <RoofDefBar
            {project}
            bind:selection
            onchange={patchEdge}
            onapplyall={applyAll}
            onlength={setLength}
            ondelete={deleteVertex}
          />
        {/if}
      </div>

      <aside>
        <div class="tabs" role="tablist">
          <button role="tab" aria-selected={dock === "results"} class:on={dock === "results"} onclick={() => (dock = "results")}><Icon name="chart" size={15} />{t("dock.results")}</button>
          <button role="tab" aria-selected={dock === "ai"} class:on={dock === "ai"} onclick={() => (dock = "ai")}>
            <Icon name="sparkle" size={15} />{t("dock.ai")}{#if pending}<i class="badge"></i>{/if}
          </button>
          <button role="tab" aria-selected={dock === "job"} class:on={dock === "job"} onclick={() => (dock = "job")}><Icon name="briefcase" size={15} />{t("dock.job")}</button>
        </div>
        <div class="dock">
          {#if dock === "results"}
            <Results {result} />
          {:else if dock === "ai"}
            <AiPanel {id} {project} {activity} onrevert={revert} {onconnect} onfocus={(c) => (focusChange = c)} />
          {:else}
            <JobPanel {project} onchange={commit} />
          {/if}
        </div>
      </aside>
    </div>

    <footer class="statusbar">
      <span class="coords">{cursor ? `X ${num(cursor.x)}   Y ${num(cursor.y)}` : "X —   Y —"}</span>
      <span>{t("status.snap")}</span>
      <span>{t("status.walls", { n: wallCount, p: project.parts.length })}</span>
      {#if result}<span>{t("res.area")}: <b>{m2(result.totals.roof_area)}</b></span>{/if}
      <span class="spacer"></span>
      <span class="save">
        {#if saveState === "saved"}<Icon name="check" size={13} />{t("editor.saved")}{:else if saveState === "saving"}{t("editor.saving")}{:else}{t("editor.dirty")}{/if}
      </span>
    </footer>
  {/if}
</div>

{#if std}
  <Modal title={t("std.title")} width={640} onclose={() => (std = null)}>
    <StandardRoofForm bind:part={std.part} />
    {#snippet footer()}
      {#if project && project.parts.length}
        <div class="mode" role="radiogroup">
          <label><input type="radio" bind:group={std!.mode} value="replace" />{t("std.replace")}</label>
          <label><input type="radio" bind:group={std!.mode} value="add" />{t("std.add")}</label>
        </div>
      {/if}
      <button class="btn" onclick={() => (std = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={!std?.part} onclick={applyStandard}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

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
    padding: 6px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--text-3);
  }
  .crumb {
    border: none;
    background: none;
    padding: 4px 6px;
    border-radius: 6px;
    color: var(--text-2);
    font-weight: 500;
  }
  .crumb:hover {
    background: var(--surface-2);
  }
  .title {
    width: 280px;
    border-color: transparent;
    background: transparent;
    font-weight: 600;
    font-size: 15px;
  }
  .title:hover {
    border-color: var(--border);
  }
  .jobno {
    padding: 2px 8px;
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text-2);
    font-size: 12px;
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .ai-pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    max-width: 420px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 99px;
    background: var(--surface);
    font-size: 12.5px;
    color: var(--text-2);
  }
  .ai-pill .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .ai-pill.active {
    border-color: color-mix(in srgb, var(--ai) 50%, var(--border));
    background: var(--ai-soft);
    color: var(--text);
  }
  .ai-pill.active .dot {
    background: var(--ai);
    animation: blink 1.2s infinite;
  }
  .ai-pill .msg {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }
  .toolbar {
    display: flex;
    gap: 2px;
    padding: 4px 8px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
  }
  .group {
    display: flex;
    gap: 2px;
    padding-right: 8px;
    margin-right: 6px;
    border-right: 1px solid var(--border);
  }
  .group:last-child {
    border-right: none;
  }
  .tool {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-width: 62px;
    height: 50px;
    padding: 0 8px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-2);
  }
  .tool span {
    font-size: 11.5px;
    white-space: nowrap;
  }
  .tool:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }
  .tool.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .tool:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .main {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .review {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    background: var(--ai-soft);
    border-bottom: 1px solid color-mix(in srgb, var(--ai) 45%, var(--border));
    color: var(--ai-strong);
  }
  .review-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    font-size: 13px;
    color: var(--text);
  }
  .review-list {
    color: var(--text-2);
    font-size: 12.5px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .btn.sm {
    height: 30px;
    padding: 0 12px;
    font-size: 13px;
  }
  .btn.accept {
    background: var(--ai);
    border-color: var(--ai);
    color: #fff;
  }
  .views {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr;
    gap: 1px;
    background: var(--border);
  }
  .views.split {
    grid-template-columns: 1fr 1fr;
  }
  .view {
    position: relative;
    min-width: 0;
    min-height: 0;
  }
  .view-label {
    position: absolute;
    z-index: 2;
    left: 10px;
    top: 8px;
    padding: 2px 8px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--surface) 85%, transparent);
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-3);
    pointer-events: none;
  }
  aside {
    width: 330px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--border);
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 6px 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .tabs button {
    position: relative;
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 36px;
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-2);
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .badge {
    position: absolute;
    top: 6px;
    right: 10px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ai);
  }
  .dock {
    flex: 1;
    overflow: auto;
    padding: 16px;
  }
  .statusbar {
    display: flex;
    align-items: center;
    gap: 18px;
    height: 26px;
    padding: 0 12px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-2);
  }
  .coords {
    min-width: 150px;
    font-variant-numeric: tabular-nums;
  }
  .save {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .mode {
    display: flex;
    gap: 14px;
    margin-right: auto;
    font-size: 13px;
  }
  .mode label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
