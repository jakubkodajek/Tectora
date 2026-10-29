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
  import DormerBar from "./DormerBar.svelte";
  import OpeningBar from "./OpeningBar.svelte";
  import ManualBar from "./ManualBar.svelte";
  import { snapToRoof, type ManualSel } from "./manual";
  import { chimneyFromRect, nonConvexOpenings, openingList, type OpeningSel } from "./openings";
  import { buildDormer, defaultParams, nearestWallAngle, roofHeightAt } from "./dormers";
  import Roof3D from "./Roof3D.svelte";
  import StandardRoofForm from "./StandardRoofForm.svelte";
  import RoundRoofForm from "./RoundRoofForm.svelte";
  import RoofCalcForm from "./RoofCalcForm.svelte";
  import { oppositeWall, spanBetween, type Arc } from "./roundroof";
  import { api, onProjectsChanged } from "./api";
  import { changedEdges, describeChange } from "./changes";
  import { m2, num } from "./format";
  import { t } from "./i18n.svelte";
  import { actorName, isFresh, live } from "./live.svelte";
  import { markReviewed, prefs, presets, reviewedAt, savePrefs } from "./settings.svelte";
  import type { ActivityItem, CalcResult, Change, Chimney, Dormer, EdgeDef, ManualFace, ManualLine, Point2, Project, RoofPart, RoofWindow, Selection } from "./types";
  import { isAi, isGable, isMarker } from "./types";

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
  let dormerSel = $state<number | null>(null);
  let placing = $state<false | "dormer" | "chimney" | "window" | "hole" | "line" | "face">(false);
  // Hand-drawn roof lines and faces (DACAD: Dachlinieneingabe, Neue Dachfläche).
  let manualSel = $state<ManualSel>(null);
  /** Points picked so far for the line or face being added. */
  let manualDraft = $state<[number, number, number][]>([]);
  $effect(() => {
    if (!manualSel || !project) return;
    const list = manualSel.kind === "line" ? project.manual_lines : project.manual_faces;
    if (manualSel.index >= (list?.length ?? 0)) manualSel = null;
  });
  $effect(() => {
    if (placing !== "line" && placing !== "face") manualDraft = [];
  });
  let openSel = $state<OpeningSel>(null);
  // Undo or an AI edit can remove the selected dormer or opening.
  $effect(() => {
    if (dormerSel !== null && (!project || dormerSel >= project.dormers.length)) dormerSel = null;
  });
  $effect(() => {
    if (!openSel) return;
    if (!project || openSel.index >= openingList(project, openSel.kind).length) openSel = null;
  });
  let cursor = $state<Point2 | null>(null);
  let showDims = $state(true);
  let activity = $state<ActivityItem[]>([]);
  let accepted = $state(untrack(() => reviewedAt(id)));
  let focusChange = $state<Change | null>(null);
  let std = $state<{ part: RoofPart | null; mode: "add" | "replace" } | null>(null);
  let plan: PlanView | undefined = $state();
  // Round-roof helper on the selected wall and the one facing it.
  let round = $state<{ part: number; a: number; b: number; span: number; eave: number; overhang: number; arc: Arc | null } | null>(null);
  const opposite = $derived(
    project && selection && !isMarker(project.parts[selection.part].edges[selection.edge])
      ? oppositeWall(project.parts[selection.part].edges, selection.edge)
      : null,
  );

  // Undo history of serialized projects; `cursor` points at the current one.
  let history: string[] = [];
  let at = $state(0);
  let historyLen = $state(0);
  // Last JSON written by us, to tell our own saves from external edits.
  let lastSaved = "";
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let calcSeq = 0;

  const snapshot = () => JSON.stringify(project);

  /** AI changes since the user's last own action or accept, newest first. */
  const pendingList = $derived.by(() => {
    const out: ActivityItem[] = [];
    for (const a of activity) {
      if (!isAi(a.actor) || a.ts_ms <= accepted) break;
      out.push(a);
    }
    return out;
  });
  const pending = $derived(pendingList.length ? pendingList[0] : null);
  // DACAD calculates no roof once a chimney or roof opening is non-convex.
  const nonConvex = $derived(project ? nonConvexOpenings(project) : []);
  const pendingChanges = $derived(pendingList.flatMap((a) => a.changes));
  const highlight = $derived.by(() => {
    if (!project) return new Set<string>();
    if (focusChange) return changedEdges([focusChange], project);
    return changedEdges(pendingChanges, project);
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
      // The store writes its own canonical form (no nulls, empty lists or
      // defaults); remember that, or the file-change event would come back
      // as an external edit and break undo.
      lastSaved = JSON.stringify(await api.loadProject(id));
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
        if (pending) dock = "ai";
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

  /** Reverts the pending AI changes from the banner, or the latest one from the AI tab. */
  async function revert(all = false) {
    try {
      clearTimeout(saveTimer);
      const oldest = pendingList[pendingList.length - 1];
      const p = all && oldest ? await api.revertChanges(id, oldest.ts_ms) : await api.revertLastChange(id);
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

  // Calculation helpers on the selected wall.
  let calc = $state<{ part: number; edge: number; patches: Map<number, Partial<EdgeDef>> | null } | null>(null);

  function applyCalc() {
    if (!project || !calc?.patches) return;
    const es = project.parts[calc.part].edges;
    for (const [k, patch] of calc.patches) Object.assign(es[k], patch);
    calc = null;
    commit();
  }

  function openRound() {
    if (!project || !selection || opposite === null) return;
    const es = project.parts[selection.part].edges;
    const e = es[selection.edge];
    round = {
      part: selection.part,
      a: selection.edge,
      b: opposite,
      span: spanBetween(es, selection.edge, opposite),
      eave: e.eave_height,
      overhang: e.overhang,
      arc: null,
    };
  }

  /** Sets both walls to the arc's pitch and breaks (DACAD: Runddach). */
  function applyRound() {
    if (!project || !round?.arc) return;
    const es = project.parts[round.part].edges;
    for (const k of [round.a, round.b]) {
      Object.assign(es[k], {
        pitch_deg: round.arc.pitch_deg,
        eave_height: round.eave,
        overhang: round.overhang,
        half_hip: null,
        breaks: round.arc.breaks.map((b) => ({ ...b })),
      });
    }
    round = null;
    commit();
  }

  function patchEdge(patch: Partial<EdgeDef>) {
    if (!project || !selection) return;
    Object.assign(project.parts[selection.part].edges[selection.edge], patch);
    commit();
  }

  function applyAll(patch: Partial<EdgeDef>) {
    if (!project || !selection) return;
    for (const e of project.parts[selection.part].edges) {
      if ("pitch_deg" in patch && (isGable(e) || isMarker(e))) continue;
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
    if (!isMarker(es[selection.edge]) && es.filter((e) => !isMarker(e)).length <= 3) return;
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

  /** Inserts a direction change (DACAD: Richtungswechsel) at the start of the selected wall. */
  function insertMarker() {
    if (!project || !selection) return;
    const es = project.parts[selection.part].edges;
    const e = es[selection.edge];
    const next = es[(selection.edge + 1) % es.length].start;
    const ang = Math.atan2(next.y - e.start.y, next.x - e.start.x) + Math.PI / 2;
    const marker: EdgeDef = {
      ...$state.snapshot(e),
      start: { ...e.start },
      half_hip: null,
      breaks: [],
      clad: false,
      pitch_deg: isGable(e) ? presets.pitch_deg : e.pitch_deg,
      direction_rad: (ang + 2 * Math.PI) % (2 * Math.PI),
    };
    es.splice(selection.edge, 0, marker);
    commit();
  }

  /** Places a gable dormer with its front's left corner at the click (DACAD: Gaube plazieren). */
  function placeDormer(x: number, y: number) {
    if (!project) return;
    placing = false;
    const angle = nearestWallAngle(project.parts, { x, y });
    const host = roofHeightAt(result, { x: x - Math.sin(angle) * 0.01, y: y + Math.cos(angle) * 0.01 });
    const eave = +((host ?? presets.eave_height) + 1).toFixed(2);
    project.dormers.push(buildDormer(defaultParams("gable", { x, y }, angle, eave)));
    selection = null;
    openSel = null;
    dormerSel = project.dormers.length - 1;
    commit();
  }

  function place(x: number, y: number) {
    if (placing === "dormer") placeDormer(x, y);
    else if (placing === "chimney") placeChimney(x, y);
    else if (placing === "window") placeWindow(x, y);
    else if (placing === "hole") placeHole(x, y);
    else if (placing === "line") placeLinePoint(x, y);
    else if (placing === "face") placeFacePoint(x, y);
  }

  // Snap radius to the calculated roof's points, m [no source: picked to
  // match a few pixels at the usual plan zoom].
  const ROOF_SNAP = 0.25;

  /** First click: the line's start; second: its end (DACAD: Dachlinieneingabe). */
  function placeLinePoint(x: number, y: number) {
    if (!project) return;
    const p = snapToRoof(result, { x, y }, ROOF_SNAP);
    if (!manualDraft.length) {
      manualDraft = [p];
      return;
    }
    const a = manualDraft[0];
    if (Math.hypot(p[0] - a[0], p[1] - a[1], p[2] - a[2]) < 1e-6) return;
    project.manual_lines ??= [];
    // DACAD's default type is a dimension line.
    project.manual_lines.push({ a, b: p, code: "M" });
    manualDraft = [];
    placing = false;
    selectManual({ kind: "line", index: project.manual_lines.length - 1 });
    commit();
  }

  /** Clicks add corners; clicking the first one again closes the face (DACAD: Neue Dachfläche). */
  function placeFacePoint(x: number, y: number) {
    if (!project) return;
    const p = snapToRoof(result, { x, y }, ROOF_SNAP);
    const first = manualDraft[0];
    if (first && manualDraft.length >= 3 && Math.hypot(p[0] - first[0], p[1] - first[1]) < ROOF_SNAP) {
      project.manual_faces ??= [];
      // DACAD asks for the pitch with 30° filled in.
      project.manual_faces.push({ points: manualDraft.map((q) => ({ x: q[0], y: q[1] })), pitch_deg: 30 });
      manualDraft = [];
      placing = false;
      selectManual({ kind: "face", index: project.manual_faces.length - 1 });
      commit();
      return;
    }
    manualDraft = [...manualDraft, p];
  }

  /** DACAD: Berechnung › Flachdach; dormers are left out of such a roof. */
  function toggleFlat() {
    if (!project) return;
    project.flat_roof = !project.flat_roof;
    commit();
  }

  function selectManual(s: ManualSel) {
    selection = null;
    dormerSel = null;
    openSel = null;
    manualSel = s;
  }

  function setManualLine(l: ManualLine) {
    if (!project || manualSel?.kind !== "line" || !project.manual_lines) return;
    project.manual_lines[manualSel.index] = l;
    commit();
  }

  function setManualFace(f: ManualFace) {
    if (!project || manualSel?.kind !== "face" || !project.manual_faces) return;
    project.manual_faces[manualSel.index] = f;
    commit();
  }

  function deleteManual() {
    if (!project || !manualSel) return;
    if (manualSel.kind === "line") project.manual_lines?.splice(manualSel.index, 1);
    else project.manual_faces?.splice(manualSel.index, 1);
    manualSel = null;
    commit();
  }

  /** Places a roof opening with its first corner at the click (DACAD: Öffnung). */
  function placeHole(x: number, y: number) {
    if (!project) return;
    placing = false;
    project.roof_openings ??= [];
    // 1 × 1 m square along the nearest wall [no source: placeholder size].
    const angle = nearestWallAngle(project.parts, { x, y });
    project.roof_openings.push(chimneyFromRect({ x, y, width: 1, depth: 1, rotation: +((angle * 180) / Math.PI).toFixed(2), height: 0 }));
    selectOpening({ kind: "hole", index: project.roof_openings.length - 1 });
    commit();
  }

  /** Places a chimney with its first corner at the click (DACAD: Schornstein). */
  function placeChimney(x: number, y: number) {
    if (!project) return;
    placing = false;
    // 0.6 m square, top 1 m above the roof there [no source: common size].
    const top = +((roofHeightAt(result, { x: x + 0.3, y: y + 0.3 }) ?? presets.eave_height) + 1).toFixed(2);
    const angle = nearestWallAngle(project.parts, { x, y });
    project.chimneys.push(chimneyFromRect({ x, y, width: 0.6, depth: 0.6, rotation: +((angle * 180) / Math.PI).toFixed(2), height: top }));
    selectOpening({ kind: "chimney", index: project.chimneys.length - 1 });
    commit();
  }

  /** Places a roof window with its lower left corner at the click (DACAD: Dachfenster). */
  function placeWindow(x: number, y: number) {
    if (!project) return;
    placing = false;
    project.windows ??= [];
    // 0.78 x 1.18 m, a common skylight size [no source: common size].
    project.windows.push({ x, y, width: 0.78, height: 1.18 });
    selectOpening({ kind: "window", index: project.windows.length - 1 });
    commit();
  }

  function selectOpening(s: OpeningSel) {
    selection = null;
    dormerSel = null;
    manualSel = null;
    openSel = s;
  }

  function setChimney(c: Chimney) {
    if (!project || !openSel || openSel.kind === "window") return;
    if (openSel.kind === "chimney") project.chimneys[openSel.index] = c;
    else if (project.roof_openings) project.roof_openings[openSel.index] = c;
    commit();
  }

  function setWindow(w: RoofWindow) {
    if (!project || openSel?.kind !== "window" || !project.windows) return;
    project.windows[openSel.index] = w;
    commit();
  }

  function deleteOpening() {
    if (!project || !openSel) return;
    if (openSel.kind === "chimney") project.chimneys.splice(openSel.index, 1);
    else if (openSel.kind === "hole") project.roof_openings?.splice(openSel.index, 1);
    else project.windows?.splice(openSel.index, 1);
    openSel = null;
    commit();
  }

  function togglePlacing(kind: "dormer" | "chimney" | "window" | "hole" | "line" | "face") {
    placing = placing === kind ? false : kind;
    drawing = null;
  }

  function setDormer(d: Dormer) {
    if (!project || dormerSel === null) return;
    project.dormers[dormerSel] = d;
    commit();
  }

  function deleteDormer() {
    if (!project || dormerSel === null) return;
    project.dormers.splice(dormerSel, 1);
    dormerSel = null;
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
    dormerSel = null;
    openSel = null;
    placing = false;
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
    } else if (e.key === "Escape" && placing) {
      placing = false;
    } else if (e.key === "Escape" && !drawing) {
      selection = null;
      dormerSel = null;
      openSel = null;
      manualSel = null;
    } else if (e.key === "Delete" && dormerSel !== null && !drawing) {
      deleteDormer();
    } else if (e.key === "Delete" && openSel && !drawing) {
      deleteOpening();
    } else if (e.key === "Delete" && manualSel && !drawing) {
      deleteManual();
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
      <button class="tool" class:on={placing === "dormer"} disabled={!project?.parts.length} onclick={() => togglePlacing("dormer")} title={t("tool.dormerHint")}><Icon name="roof" /><span>{t("tool.dormer")}</span></button>
      <button class="tool" class:on={placing === "chimney"} disabled={!project?.parts.length} onclick={() => togglePlacing("chimney")} title={t("tool.chimneyHint")}><Icon name="chimney" /><span>{t("tool.chimney")}</span></button>
      <button class="tool" class:on={placing === "window"} disabled={!project?.parts.length} onclick={() => togglePlacing("window")} title={t("tool.windowHint")}><Icon name="window" /><span>{t("tool.window")}</span></button>
      <button class="tool" class:on={placing === "hole"} disabled={!project?.parts.length} onclick={() => togglePlacing("hole")} title={t("tool.holeHint")}><Icon name="hole" /><span>{t("tool.hole")}</span></button>
      <button class="tool" class:on={placing === "line"} disabled={!result} onclick={() => togglePlacing("line")} title={t("tool.lineHint")}><Icon name="line" /><span>{t("tool.line")}</span></button>
      <button class="tool" class:on={placing === "face"} disabled={!result} onclick={() => togglePlacing("face")} title={t("tool.faceHint")}><Icon name="face" /><span>{t("tool.face")}</span></button>
      <button class="tool" class:on={!!project?.flat_roof} disabled={!project?.parts.length} onclick={toggleFlat} title={t("tool.flatHint")}><Icon name="flat" /><span>{t("tool.flat")}</span></button>
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
              <strong>
                {t("review.title", { who: actorName(pending.actor) })}{#if pendingList.length > 1}<span class="steps">{t("review.steps", { n: pendingList.length })}</span>{/if}
              </strong>
              <span>{pendingList.map((a) => a.summary).reverse().join(" → ")}</span>
              {#if pendingChanges.length}
                <span class="review-list">
                  {pendingChanges.slice(0, 4).map((c) => describeChange(c, project!.parts.length > 1)).join(" · ")}{pendingChanges.length > 4 ? ` · ${t("review.more", { n: pendingChanges.length - 4 })}` : ""}
                </span>
              {/if}
            </div>
            <button class="btn sm" onclick={() => revert(true)}><Icon name="undo" size={14} />{pendingList.length > 1 ? t("review.revertAll") : t("ai.revert")}</button>
            <button class="btn sm accept" onclick={accept}><Icon name="check" size={14} />{t("review.accept")}</button>
          </div>
        {/if}
        {#if nonConvex.length}
          <div class="warn-bar" role="alert">
            <Icon name="warning" size={18} />
            <span>{t("opening.nonConvexAll", { n: nonConvex.length })}</span>
            <button class="btn sm" onclick={() => (openSel = nonConvex[0])}>{t("opening.show")}</button>
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
                bind:dormerSel
                bind:openSel
                bind:manualSel
                {manualDraft}
                {placing}
                onplace={place}
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
        {:else if dormerSel !== null}
          <DormerBar {project} {result} bind:index={dormerSel} onchange={setDormer} ondelete={deleteDormer} />
        {:else if openSel}
          <OpeningBar {project} bind:sel={openSel} onchimney={setChimney} onwindow={setWindow} ondelete={deleteOpening} />
        {:else if manualSel}
          <ManualBar {project} bind:sel={manualSel} online={setManualLine} onface={setManualFace} ondelete={deleteManual} />
        {:else}
          <RoofDefBar
            {project}
            bind:selection
            onchange={patchEdge}
            onapplyall={applyAll}
            onlength={setLength}
            ondelete={deleteVertex}
            onmarker={insertMarker}
            onround={opposite !== null ? openRound : undefined}
            oncalc={() => selection && (calc = { part: selection.part, edge: selection.edge, patches: null })}
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
            <AiPanel {id} {project} {activity} onrevert={() => revert(false)} {onconnect} onfocus={(c) => (focusChange = c)} />
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

{#if calc && project}
  <Modal title={t("calc.title")} width={640} onclose={() => (calc = null)}>
    <RoofCalcForm part={project.parts[calc.part]} edge={calc.edge} {result} bind:patches={calc.patches} />
    {#snippet footer()}
      <button class="btn" onclick={() => (calc = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={!calc?.patches} onclick={applyCalc}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

{#if round}
  <Modal title={t("round.title")} width={600} onclose={() => (round = null)}>
    <RoundRoofForm span={round.span} bind:eave={round.eave} bind:overhang={round.overhang} bind:arc={round.arc} />
    {#snippet footer()}
      <button class="btn" onclick={() => (round = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={!round?.arc} onclick={applyRound}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

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
  .warn-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    background: color-mix(in srgb, var(--danger) 12%, var(--surface));
    border-bottom: 1px solid color-mix(in srgb, var(--danger) 45%, var(--border));
    color: var(--danger);
    font-size: 13.5px;
  }
  .warn-bar span {
    flex: 1;
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
  .steps {
    margin-left: 8px;
    padding: 1px 7px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--ai) 20%, transparent);
    font-size: 11.5px;
    font-weight: 600;
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
