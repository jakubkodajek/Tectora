<script lang="ts">
  // Design editor laid out like DACAD (toolbar, plan + 3D windows, roof
  // definition bar, status bar) with a side dock for results, AI activity
  // and job data.
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import AiPanel from "./AiPanel.svelte";
  import DrawBar from "./DrawBar.svelte";
  import Icon from "./Icon.svelte";
  import JobPanel from "./JobPanel.svelte";
  import LanguagePicker from "./LanguagePicker.svelte";
  import Modal from "./Modal.svelte";
  import ExportDialog from "./ExportDialog.svelte";
  import PlanView from "./PlanView.svelte";
  import Results from "./Results.svelte";
  import Breakdown from "./Breakdown.svelte";
  import RoofDefBar from "./RoofDefBar.svelte";
  import DormerBar from "./DormerBar.svelte";
  import DormerDialog from "./DormerDialog.svelte";
  import OpeningBar from "./OpeningBar.svelte";
  import ManualBar from "./ManualBar.svelte";
  import { commitFocused } from "./commitfocus";
  import { textfield } from "./textfield";
  import ContextMenu, { type MenuEntry, type MenuState } from "./ContextMenu.svelte";
  import { mergeDroppedCorner, moveCorner } from "./corners";
  import { followRoof, itemCentre, itemExists, moveItem, rotateItem, deleteItem, transformItem, type ItemRef, type PlanTarget } from "./items";
  import { snapToRoof, type ManualSel } from "./manual";
  import { describeCalcError, describeError, describeProblem, toAppError, type Described } from "./errors";
  import { info, notify, notifyError } from "./notify.svelte";
  import { chimneyFromRect, nonConvexOpenings, openingList, type OpeningSel } from "./openings";
  import { buildDormer, defaultParams, nearestWallAngle, parseDormer, roofHeightAt, type DormerParams } from "./dormers";
  import Roof3D from "./Roof3D.svelte";
  import StandardRoofForm from "./StandardRoofForm.svelte";
  import RoundRoofForm from "./RoundRoofForm.svelte";
  import RoofCalcForm from "./RoofCalcForm.svelte";
  import TransformForm from "./TransformForm.svelte";
  import { offsetWall, transformPart, translation, type TransformMode, type TransformOp } from "./transform";
  import { oppositeWall, spanBetween, type Arc } from "./roundroof";
  import { api, onProjectsChanged } from "./api";
  import { changedEdges, changedItems, describeChange, describeFigure, ghostOf } from "./changes";
  import { merge3 } from "./merge";
  import { isTyping, releaseLeaveGuard, setLeaveGuard } from "./leave";
  import { PendingEdits, exclusive } from "./inflight";
  import { m2, num } from "./format";
  import { i18n, t } from "./i18n.svelte";
  import { hscroll } from "./hscroll";
  import { partLabel } from "./partname";
  import { exportTitle } from "./exportlist";
  import { tabIds, tablist } from "./tabs";
  import { actorName, entrySummary, isFresh, live, liveText } from "./live.svelte";
  import { prefs, presets, reviewedAt, savePrefs } from "./settings.svelte";
  import type { ActivityItem, ReviewStatus, CalcBreakdown, CalcResult, Change, Chimney, Dormer, EdgeDef, ManualFace, ManualLine, PlanLabel, PlanMark, Point2, Project, RoofPart, RoofWindow, Selection } from "./types";
  import { GABLE_PITCH, isAi, isGable, isMarker } from "./types";

  let {
    id: idProp,
    onback,
    onconnect,
  }: { id: string; onback: () => void; onconnect: () => void } = $props();
  // The job this editor instance edits (App re-creates the editor for
  // another job). Props are live: when App opens another job or goes back,
  // `idProp` already has the new value while this instance is torn down,
  // and the save on destroy must still write to this job's file.
  const id = untrack(() => idProp);

  let project = $state<Project | null>(null);
  let result = $state<CalcResult | null>(null);
  let selection = $state<Selection>(null);
  let saveState = $state<"saved" | "saving" | "dirty" | "failed">("saved");
  /** Why the project could not be opened (shown instead of the editor). */
  let loadError = $state<Described | null>(null);
  let loadErrorCode = $state<string>("");
  let dock = $state<"results" | "breakdown" | "ai" | "job">("results");
  const dockIds = tabIds("dock");

  // The toolbar drops its labels (icons + tooltips) whenever the labelled
  // buttons do not fit the window, e.g. German at 1360 or 1440 px, and takes
  // them back once the window is wide enough for the width they needed.
  let toolbarEl: HTMLDivElement | undefined = $state();
  let compactTools = $state(false);
  let labelledWidth = 0;
  function fitToolbar() {
    const el = toolbarEl;
    if (!el) return;
    if (!compactTools) {
      if (el.scrollWidth > el.clientWidth + 1) {
        labelledWidth = el.scrollWidth;
        compactTools = true;
      }
    } else if (el.clientWidth >= labelledWidth) {
      compactTools = false;
      tick().then(fitToolbar);
    }
  }
  $effect(() => {
    const el = toolbarEl;
    if (!el) return;
    const ro = new ResizeObserver(() => fitToolbar());
    ro.observe(el);
    return () => ro.disconnect();
  });
  $effect(() => {
    // Labels of another language need another width: measure again.
    void i18n.locale;
    untrack(() => {
      compactTools = false;
      labelledWidth = 0;
      tick().then(fitToolbar);
    });
  });
  /** The calculation breakdown while its tab is open, the face or line it points at, and face numbers for the plan. */
  let breakdown = $state<CalcBreakdown | null>(null);
  let planMark = $state<PlanMark>(null);
  const faceLabels = $derived.by((): PlanLabel[] => {
    if (!breakdown || dock !== "breakdown" || !dockOpen) return [];
    const many = breakdown.parts.length > 1;
    return breakdown.parts.flatMap((p, pi) =>
      p.faces.map((f) => {
        const n = f.polygon.length || 1;
        return {
          x: f.polygon.reduce((s, v) => s + v[0], 0) / n,
          y: f.polygon.reduce((s, v) => s + v[1], 0) / n,
          text: many ? `${pi + 1}.${f.no}` : `${f.no}`,
        };
      }),
    );
  });
  /** Narrow windows only: whether the side panel is expanded over the views. */
  let dockOpen = $state(false);

  function showDock(d: typeof dock) {
    dockOpen = dock !== d || !dockOpen;
    dock = d;
  }
  let drawing = $state<Point2[] | null>(null);
  let dormerSel = $state<number | null>(null);
  /** The dormer dialog: a new dormer (index null) or dormer `index`. */
  let dormerDlg = $state<{ index: number | null; params: DormerParams } | null>(null);
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
  let exporting = $state(false);
  let activity = $state<ActivityItem[]>([]);
  /** AI changes waiting for review, as the library keeps them (shared with the agents). */
  let review = $state<ReviewStatus | null>(null);
  /** Show the state before the pending AI changes dashed in the plan. */
  let showBefore = $state(true);
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
  /** The jobs folder the job was opened from (see `save`). */
  let libraryDir: string | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  /** Saves in flight, and edits made so far (to tell whether a save is still current). */
  let inFlight = 0;
  let edits = 0;
  /** A file-change event came in while a save was running. */
  let reloadWanted = false;
  /** Saves started so far: a read that a save overtook is stale. */
  let saveGen = 0;
  /** Saves run one after another: two from the same base would conflict
      with each other, and the merge would drop the later local value. */
  let saveQueue: Promise<unknown> = Promise.resolve();
  /** Outside saves merged in a row before a save gives up (`save_conflict`). */
  const SAVE_TRIES = 5;
  let calcSeq = 0;
  /** Field handlers that ask the core before they change the job: leaving waits for them (ERR-004). */
  const pendingEdits = new PendingEdits();

  const snapshot = () => JSON.stringify(project);

  /** AI changes since the user's last own action or accept, newest first. */
  const pendingList = $derived(review ? activity.filter((a) => isAi(a.actor) && a.ts_ms >= review!.since_ts_ms) : []);
  const pending = $derived(pendingList.length ? pendingList[0] : null);
  // DACAD calculates no roof once a chimney or roof opening is non-convex.
  const nonConvex = $derived(project ? nonConvexOpenings(project) : []);
  /** DACAD's calculation error, if it gave no roof. The non-convex case
      has its own bar above, with a button to show the opening. */
  const calcError = $derived.by(() => {
    const e = result?.parts.find((p) => p.geometry.error)?.geometry.error ?? null;
    return e && !(e.kind === "non_convex_opening" && nonConvex.length) ? e : null;
  });
  /** The core gave no roof for a part (the non-convex case too): its figures
      are zero, so no export and no offer or order from it (ERR-003). */
  const roofError = $derived(result?.parts.find((p) => p.geometry.error)?.geometry.error ?? null);
  // No result at all: the calculation failed as a whole (too many breaks,
  // a value out of range, an internal error) or has not come back yet (DR-002).
  const blockedReason = $derived(!result || roofError ? t("tool.exportBlocked") : null);
  $effect(() => {
    if (roofError) exporting = false;
  });
  const pendingChanges = $derived(pendingList.flatMap((a) => a.changes));
  const highlight = $derived.by(() => {
    if (!project) return new Set<string>();
    if (focusChange) return changedEdges([focusChange], project);
    return changedEdges(pendingChanges, project);
  });
  const highlightItems = $derived(changedItems(focusChange ? [focusChange] : pendingChanges));
  const ghost = $derived(
    project && review && showBefore ? ghostOf(focusChange ? [focusChange] : pendingChanges, review.before, project, result) : null,
  );
  const wallCount = $derived(project?.parts.reduce((n, p) => n + p.edges.length, 0) ?? 0);

  async function recalc() {
    if (!project) return;
    const seq = ++calcSeq;
    try {
      const r = await api.calc($state.snapshot(project) as Project);
      if (seq === calcSeq) result = r;
    } catch (e) {
      if (seq === calcSeq) result = null;
      notifyError(e, "calc");
    }
  }

  async function refreshActivity() {
    try {
      const [a, r] = await Promise.all([api.projectActivity(id), api.reviewStatus(id).catch(() => null)]);
      activity = a;
      review = r;
    } catch {
      activity = [];
      review = null;
    }
  }

  /** Accepts done in an older version were kept in this browser only. */
  async function migrateLocalReview() {
    const local = reviewedAt(id);
    if (!review || local < review.since_ts_ms) return;
    try {
      await api.acceptChanges(id, local);
      await refreshActivity();
    } catch {
      // Stays pending; the user can accept again.
    }
  }

  function scheduleSave() {
    saveState = "dirty";
    edits++;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 500);
  }

  /** Writes the job; resolves to whether the edits are on disk (errors are shown, never thrown). */
  function save(): Promise<boolean> {
    if (!project) return Promise.resolve(false);
    clearTimeout(saveTimer);
    saveState = "saving";
    inFlight++;
    saveGen++;
    const run = saveQueue.then(saveNow).finally(() => {
      inFlight--;
      if (!inFlight && reloadWanted) {
        reloadWanted = false;
        load(false);
      }
    });
    saveQueue = run.catch(() => {});
    return run;
  }

  async function saveNow(): Promise<boolean> {
    if (!project) return false;
    saveState = "saving";
    const seq = edits;
    try {
      // The jobs folder was switched under the open job (settings, or an AI
      // over MCP): the same id there is another job, never write over it.
      if (libraryDir !== null && (await api.projectsDir()) !== libraryDir) {
        throw { code: "library_changed", id, detail: `${libraryDir} -> ${await api.projectsDir()}` };
      }
      for (let tries = 0; ; tries++) {
        const written = JSON.parse(snapshot());
        if (lastSaved) {
          // Written only if the file is still the one we last read or wrote
          // (checked and written in one step by the store). An external edit
          // (AI agent, CLI) saved since then comes back instead: merge it in
          // and try again, never write over it. A file saved by a newer
          // Tectora or unreachable is not overwritten: the error goes to the
          // catch below and the edits stay in memory. A file deleted outside
          // the app is written again, and a damaged one is repaired with the
          // editor's good copy, as the store allows (ERR-018).
          const r = await api.saveProjectChecked(id, written, JSON.parse(lastSaved));
          if (r.status === "conflict") {
            if (tries + 1 >= SAVE_TRIES) throw { code: "save_conflict", id };
            project = merge3(JSON.parse(lastSaved), $state.snapshot(project) as Project, r.current);
            lastSaved = JSON.stringify(r.current);
            pushHistory();
            recalc();
            refreshActivity();
            continue;
          }
        } else {
          await api.saveProject(id, written);
        }
        // The store writes its own canonical form (no nulls, empty lists or
        // defaults); remember that, or the file-change event would come back
        // as an external edit and break undo. Not read back from the file: an
        // AI edit saved right after ours would then pass for our own.
        lastSaved = JSON.stringify(await api.normalizeProject(written));
        // An edit made while this save was running is still unsaved.
        saveState = edits === seq ? "saved" : "dirty";
        return true;
      }
    } catch (e) {
      // Keep the edits in memory; the status bar offers to try again.
      saveState = "failed";
      notifyError(e, "save");
      return false;
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
    const gen = saveGen;
    try {
      const p = await api.loadProject(id);
      const json = JSON.stringify(p);
      await refreshActivity();
      if (!initial && inFlight) {
        // A save started while reading: this read may be older than it.
        reloadWanted = true;
        return;
      }
      if (!initial && gen !== saveGen) {
        // A save started and finished while reading: this read may be older
        // than it and would pass for an outside edit, putting the job back
        // to before the user's last edit. Read again.
        load(false);
        return;
      }
      if (!initial && json === lastSaved) return;
      // Indices in an open menu or a drag in progress refer to the old project.
      menu = null;
      dragBase = null;
      // An external edit while local edits are not saved yet (waiting for
      // the timer, or a save that failed): keep both (the external one wins
      // where both changed the same value). The state stays "dirty" or
      // "failed", so the merged edits are still to be saved (ERR-002).
      const merged = !initial && (saveState === "dirty" || saveState === "failed") && project && lastSaved;
      project = merged ? merge3(JSON.parse(lastSaved), $state.snapshot(project) as Project, p) : p;
      lastSaved = json;
      if (initial) {
        await migrateLocalReview();
        history = [snapshot()];
        at = 0;
        historyLen = 1;
        if (p.parts.length === 0) drawing = [];
        if (pending) dock = "ai";
      } else {
        // External change (AI agent, CLI): keep it undoable and show it.
        // Merged local edits are still to be saved (the timer is running).
        pushHistory();
        // Not while the user types in the dock: switching the tab would
        // remove the field and the typed text with it (UI-002).
        if (pending && !typingInDock()) dock = "ai";
      }
      recalc();
    } catch (e) {
      if (initial || !project) {
        loadError = describeError(e, "load");
        loadErrorCode = toAppError(e).code;
      } else if (toAppError(e).code === "not_found") {
        // Deleted outside the app while open: keep the edits, offer to save.
        saveState = "failed";
        notifyError(e, "load");
      } else {
        notifyError(e, "load");
      }
    }
  }

  let dockEl: HTMLElement | undefined = $state();
  /** A text field in the side panel has the focus. */
  function typingInDock(): boolean {
    const a = document.activeElement;
    return !!dockEl && !!a && dockEl.contains(a) && (a instanceof HTMLInputElement || a instanceof HTMLTextAreaElement || a instanceof HTMLSelectElement);
  }

  /** Follow mode must not take the user out of unfinished work (UX-001). */
  function busy(): boolean {
    if ((drawing?.length ?? 0) > 0 || placing !== false || manualDraft.length > 0) return true;
    if (dragBase || menu) return true;
    return isTyping(document.activeElement);
  }

  let unlisten: (() => void) | undefined;
  onMount(() => {
    api.projectsDir().then((d) => (libraryDir = d), () => {});
    load(true);
    onProjectsChanged((ids) => {
      if (!ids.includes(id)) return;
      // While our own save runs, `lastSaved` is not yet up to date, so the
      // file would look like an external edit and replace newer local edits.
      // Check again once the save is done.
      if (inFlight) reloadWanted = true;
      else load(false);
    }).then((u) => (unlisten = u));
  });
  /** Saves pending edits before the job is left; asks when they cannot be saved. */
  async function leaveGuard(mode: "ask" | "quiet"): Promise<boolean> {
    // A footprint being drawn is not in the job yet: leaving drops it, so
    // ask as Escape does (UX-013). Follow mode (not the user's) never drops it.
    const pts = drawing?.length ?? 0;
    if (pts >= 2) {
      if (mode === "quiet") return false;
      // Tauri's dialog plugin makes confirm asynchronous: always await it.
      if (!(await confirm(t("draw.confirmCancel", { n: pts })))) return false;
    }
    // A value still being typed is part of the job: store it first.
    commitFocused();
    await tick();
    // The stored value may still be with the core (window size check,
    // building height): it marks the job dirty only when it comes back.
    await pendingEdits.settled();
    await tick();
    if (saveState === "dirty" || saveState === "saving" || saveState === "failed") await save();
    if (saveState !== "failed") return true;
    return mode === "ask" && (await confirm(t("editor.leaveUnsaved")));
  }
  setLeaveGuard(leaveGuard, () => saveState !== "saved", busy);

  onDestroy(() => {
    unlisten?.();
    releaseLeaveGuard(leaveGuard);
    if (saveState === "dirty" || saveState === "failed") save();
  });

  /** Accept or Revert is running: a second press (double-click) must not run it twice (UI-002). */
  let reviewBusy = $state(false);
  const reviewAction = exclusive((run: () => Promise<void>) => run(), (b) => (reviewBusy = b));
  const accept = () => reviewAction(acceptNow);
  /** Reverts the pending AI changes from the banner, or the latest one from the AI tab. */
  const revert = (all = false) => reviewAction(() => revertNow(all));

  async function acceptNow() {
    if (!pending) return;
    const ts = pending.ts_ms;
    review = null;
    focusChange = null;
    try {
      await api.acceptChanges(id, ts);
    } catch (e) {
      notifyError(e, "save");
    }
    await refreshActivity();
  }

  async function revertNow(all: boolean) {
    // Reverting several AI steps at once is one click next to Accept, and
    // after a reopen it cannot be undone: ask first, naming the steps (UX-011).
    const steps = pendingList.length;
    if (all && steps > 1) {
      // Tauri's dialog plugin makes confirm asynchronous: always await it.
      if (!(await confirm(t("review.confirmRevertAll", { n: steps })))) return;
    }
    // AI steps that came in while the question was open are reverted too
    // (the user asked for all), and said so (DR-003).
    const extra = all && steps > 1 ? pendingList.length - steps : 0;
    // Edits not on disk yet would be lost: the store keeps only the edits in
    // the file, merging the revert onto them (ERR-001).
    commitFocused();
    await tick();
    if (saveState !== "saved" && !(await save())) return;
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
      focusChange = null;
      if (extra > 0) info(t("review.revertedMore", { n: extra }));
      await refreshActivity();
    } catch (e) {
      notifyError(e, "revert");
      await refreshActivity();
    }
  }

  // Calculation helpers on the selected wall.
  let calc = $state<{ part: number; edge: number; patches: Map<number, Partial<EdgeDef>> | null } | null>(null);

  /** The move / rotate / mirror / copy dialog for the selected part (DACAD: Bearbeiten). */
  let xform = $state<{ part: number; edge: number | null; mode: TransformMode; op: TransformOp | null } | null>(null);

  function openTransform() {
    if (!project || !selection) return;
    xform = { part: selection.part, edge: selection.edge, mode: "move", op: null };
  }

  function applyTransform() {
    if (!project || !xform?.op) return;
    const op = xform.op;
    if (op.kind === "wall") {
      if (!offsetWall(project.parts[xform.part], op.edge, op.dist)) return;
    } else {
      const parts = transformPart(project, xform.part, op.map, op.copies);
      if (parts.length) selection = { part: parts[parts.length - 1], edge: 0 };
    }
    xform = null;
    commit();
    setTimeout(() => plan?.fit(), 0);
  }

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

  /** Makes the current result the job's stored result (the offer's figures). */
  const storeResult = () => pendingEdits.track(storeResultNow());
  async function storeResultNow() {
    // Zero figures of a roof the core refused are no offer (DR-001).
    if (blockedReason) return;
    if (!project) return;
    try {
      project.result_snapshot = await api.takeSnapshot(JSON.parse(JSON.stringify(project)));
      commit();
    } catch (e) {
      notifyError(e, "save");
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
      if ("pitch_deg" in patch && (isGable(e) || isMarker(e))) continue;
      Object.assign(e, patch);
    }
    commit();
  }

  /**
   * The bar's "Building height": every wall of the selected part at `h`, its
   * breaks, half-hips and the objects on it moved with it, in one change.
   * The core's operation, the same as MCP set_part_height.
   */
  const setPartHeight = (h: number) => pendingEdits.track(setPartHeightNow(h));
  async function setPartHeightNow(h: number) {
    if (!project || !selection) return;
    const pi = selection.part;
    const part = JSON.stringify($state.snapshot(project.parts[pi]));
    let next: Project;
    try {
      next = await api.setPartHeight($state.snapshot(project) as Project, pi, h);
    } catch (e) {
      notifyError(e, "calc");
      return;
    }
    // The project may have changed meanwhile (an AI save): apply only to the same part.
    // (Compared by value: a save in between may hand back equal but new objects.)
    if (!project || !project.parts[pi] || JSON.stringify($state.snapshot(project.parts[pi])) !== part) return;
    project.parts = next.parts;
    project.dormers = next.dormers;
    project.chimneys = next.chimneys;
    if (project.manual_lines?.length) project.manual_lines = next.manual_lines ?? [];
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

  /**
   * Delete / Backspace with a wall selected: the key is easy to hit while
   * stepping through walls (Backspace meant for a field), and it changes the
   * footprint, so it asks first (UX-006). The bar's trash button does not.
   */
  async function deleteVertexByKey() {
    if (!project || !selection) return;
    const sel = selection;
    const part = project.parts[sel.part];
    const es = part?.edges;
    if (!es?.[sel.edge] || (!isMarker(es[sel.edge]) && es.filter((e) => !isMarker(e)).length <= 3)) return;
    const msg = isMarker(es[sel.edge]) ? t("edge.confirmDeleteMarker") : t("edge.confirmDeleteVertex", { n: sel.edge + 1, name: partLabel(part.name, sel.part) });
    // Tauri's dialog plugin makes confirm asynchronous: always await it.
    if (!(await confirm(msg))) return;
    // The project or the selection may have changed while the dialog was open.
    if (project?.parts[sel.part] !== part || selection?.part !== sel.part || selection?.edge !== sel.edge) return;
    deleteVertex();
  }

  function moveVertex(part: number, v: number, x: number, y: number) {
    if (!project) return;
    const es = project.parts[part]?.edges;
    if (!es?.[v] || !Number.isFinite(x) || !Number.isFinite(y)) return;
    const p = es[v].start;
    if (p.x === x && p.y === y) return;
    moveCorner(es, v, x, y);
    recalc();
  }

  /** End of a corner drag: a corner dropped onto its neighbour merges into it. */
  function endVertexDrag(part: number, v: number) {
    const es = project?.parts[part]?.edges;
    if (es && mergeDroppedCorner(es, v) && selection?.part === part) {
      selection = { part, edge: Math.min(v, es.length - 1) };
    }
    commit();
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
    // As DACAD's dormer dialog: the dormer is placed when its window says OK.
    dormerDlg = { index: null, params: defaultParams("gable", { x, y }, angle, eave) };
  }

  function editDormer() {
    if (!project || dormerSel === null) return;
    const p = parseDormer(project.dormers[dormerSel]);
    if (p.kind !== "general") dormerDlg = { index: dormerSel, params: p };
  }

  function applyDormerDialog() {
    if (!project || !dormerDlg) return;
    const d = buildDormer($state.snapshot(dormerDlg.params) as DormerParams);
    const i = dormerDlg.index;
    if (i === null || i >= project.dormers.length) {
      project.dormers.push(d);
      dormerSel = project.dormers.length - 1;
    } else {
      d.host_wall = project.dormers[i].host_wall;
      project.dormers[i] = d;
      dormerSel = i;
    }
    selection = null;
    openSel = null;
    dormerDlg = null;
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

  /**
   * A manual face drawn where a building was meant: replace it with a part of
   * the same outline with the new-roof defaults (pitch, eave height,
   * overhang) so its roof is calculated. The core's operation, the same as
   * MCP face_to_part.
   */
  const faceToPart = () => pendingEdits.track(faceToPartNow());
  async function faceToPartNow() {
    if (!project || manualSel?.kind !== "face" || !project.manual_faces) return;
    const index = manualSel.index;
    const face = project.manual_faces[index];
    if (!face || face.points.length < 3) return;
    const n = project.parts.length;
    const name = n === 0 ? t("editor.mainPart") : t("editor.partN", { n: n + 1 });
    let next: Project;
    try {
      next = await api.faceToPart($state.snapshot(project) as Project, index, name, presets.pitch_deg, presets.eave_height, presets.overhang);
    } catch (e) {
      notifyError(e, "calc");
      return;
    }
    // The project may have changed meanwhile (an AI save): apply only to the same face.
    if (!project?.manual_faces || project.manual_faces[index] !== face) return;
    project.manual_faces = next.manual_faces;
    project.parts = next.parts;
    manualSel = null;
    selection = { part: n, edge: 0 };
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

  const setWindow = (w: RoofWindow) => pendingEdits.track(setWindowNow(w));
  async function setWindowNow(w: RoofWindow) {
    if (!project || openSel?.kind !== "window" || !project.windows) return;
    const index = openSel.index;
    // A window larger than its face would make the face area negative
    // (MATH-001): refused with the reason, as MCP set_openings refuses it.
    const windows = project.windows.map((x, i) => (i === index ? w : x));
    try {
      const big = await api.checkWindows({ ...$state.snapshot(project), windows } as Project);
      if (big) {
        notify({
          kind: "error",
          code: "window_too_large",
          title: t("opening.tooLarge.title"),
          message: t("opening.tooLarge", { n: big.index + 1, a: num(big.area), f: num(big.face_area) }),
        });
        // Same values, new object: the fields show the stored size again.
        const kept = project?.windows?.[index];
        if (kept && project?.windows) project.windows[index] = { ...kept };
        return;
      }
    } catch (e) {
      notifyError(e, "calc");
      return;
    }
    if (!project?.windows || project.windows.length !== windows.length) return;
    project.windows[index] = w;
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

  /** The agent is working on this job right now (the pill lights up only then). */
  const aiHere = $derived(isFresh() && live.status?.project_id === id);

  // ---- Objects edited directly in the plan and the 3D view ------------

  /** The selected single object, if any. */
  const selectedItem = $derived.by((): ItemRef | null => {
    if (dormerSel !== null) return { kind: "dormer", index: dormerSel };
    if (openSel) return { kind: openSel.kind, index: openSel.index };
    if (manualSel) return { kind: manualSel.kind, index: manualSel.index };
    return null;
  });

  function selectItem(it: ItemRef | null) {
    selection = null;
    dormerSel = it?.kind === "dormer" ? it.index : null;
    openSel = it && (it.kind === "chimney" || it.kind === "window" || it.kind === "hole") ? { kind: it.kind, index: it.index } : null;
    manualSel = it && (it.kind === "line" || it.kind === "face") ? { kind: it.kind, index: it.index } : null;
  }

  /** The project and roof when a drag started; each step maps from these. */
  let dragBase: { key: string; project: string; result: CalcResult | null; from: Point2 | null } | null = null;

  function dragObject(target: PlanTarget, dx: number, dy: number, end: boolean) {
    if (!project || (target.type !== "item" && target.type !== "part")) return;
    const key = JSON.stringify(target);
    if (!dragBase || dragBase.key !== key) {
      dragBase = { key, project: snapshot(), result, from: target.type === "item" ? itemCentre(project, target.item) : null };
    }
    const base: Project = JSON.parse(dragBase.project);
    if (target.type === "item") {
      const it = target.item;
      if (!itemExists(base, it)) return;
      moveItem(base, it, dx, dy);
      if (end && dragBase.from) {
        followRoof(base, it, dragBase.from, { x: dragBase.from.x + dx, y: dragBase.from.y + dy }, dragBase.result);
      }
      project = base;
      selectItem(it);
    } else {
      if (!base.parts[target.part]) return;
      transformPart(base, target.part, translation(dx, dy));
      const sel = selection;
      project = base;
      selection = sel;
    }
    if (end) {
      dragBase = null;
      commit();
    } else {
      recalc();
    }
  }

  /** Where a copy goes: beside the original, its own size plus 30 cm away
   *  (along a dormer's front, across for everything else) [no source: editor choice]. */
  function copyOffset(it: ItemRef): Point2 {
    if (!project) return { x: 0.5, y: 0 };
    if (it.kind === "dormer") {
      const d = project.dormers[it.index];
      const s = d.edges[0]?.start ?? d.end;
      const len = Math.hypot(d.end.x - s.x, d.end.y - s.y);
      if (len > 1e-6) {
        const k = (len + 0.3) / len;
        return { x: +((d.end.x - s.x) * k).toFixed(3), y: +((d.end.y - s.y) * k).toFixed(3) };
      }
    }
    const pts: Point2[] =
      it.kind === "chimney" ? project.chimneys[it.index].corners
      : it.kind === "hole" ? (project.roof_openings ?? [])[it.index]?.corners ?? []
      : it.kind === "face" ? (project.manual_faces ?? [])[it.index]?.points ?? []
      : it.kind === "line" ? (() => { const l = project.manual_lines![it.index]; return [{ x: l.a[0], y: l.a[1] }, { x: l.b[0], y: l.b[1] }]; })()
      : [];
    const w = it.kind === "window" ? project.windows![it.index].width : pts.length ? Math.max(...pts.map((p) => p.x)) - Math.min(...pts.map((p) => p.x)) : 0.5;
    return { x: +(Math.max(w, 0.2) + 0.3).toFixed(3), y: 0 };
  }

  function duplicateItem(it: ItemRef) {
    if (!project || !itemExists(project, it)) return;
    const off = copyOffset(it);
    const from = itemCentre(project, it);
    const before = result;
    const [copy] = transformItem(project, it, translation(off.x, off.y), 1);
    if (!copy) return;
    if (from) followRoof(project, copy, from, { x: from.x + off.x, y: from.y + off.y }, before);
    selectItem(copy);
    commit();
  }

  function turnItem(it: ItemRef, deg: number) {
    if (!project || !rotateItem(project, it, deg).length) return;
    commit();
  }

  /** Turns a chimney or opening so its first side runs along the nearest wall. */
  function alignItem(it: ItemRef) {
    if (!project || (it.kind !== "chimney" && it.kind !== "hole")) return;
    const c = (it.kind === "chimney" ? project.chimneys : project.roof_openings ?? [])[it.index];
    const centre = itemCentre(project, it);
    if (!c || !centre || c.corners.length < 2) return;
    const [a, b] = c.corners;
    const now = (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI;
    const want = (nearestWallAngle(project.parts, centre) * 180) / Math.PI;
    // The smallest turn that lines the side up (any quarter turn of a square is the same).
    let d = (((want - now) % 90) + 90) % 90;
    if (d > 45) d -= 90;
    if (Math.abs(d) < 1e-6) return;
    turnItem(it, +d.toFixed(4));
  }

  function removeItem(it: ItemRef) {
    if (!project || !itemExists(project, it)) return;
    deleteItem(project, it);
    selectItem(null);
    commit();
  }

  /** Puts the keyboard into the selected object's bar (its properties). */
  async function focusProperties() {
    await tick();
    document.querySelector<HTMLInputElement>(".editor .main .bar input:not(:disabled)")?.focus();
  }

  let menu = $state<MenuState | null>(null);

  function itemTitle(it: ItemRef): string {
    const n = it.index + 1;
    return {
      dormer: t("plan.dormer", { n }),
      chimney: t("opening.chimneyN", { n }),
      window: t("opening.windowN", { n }),
      hole: t("opening.holeN", { n }),
      line: t("manual.lineN", { n }),
      face: t("manual.faceN", { n }),
    }[it.kind];
  }

  /** "Add … here" entries, for a press on the roof or next to it. */
  function placeEntries(at: Point2): MenuEntry[] {
    if (!project?.parts.length) return [];
    const go = (kind: "dormer" | "chimney" | "window" | "hole") => () => {
      placing = kind;
      place(at.x, at.y);
    };
    return [
      { label: t("menu.placeChimney"), icon: "chimney", action: go("chimney") },
      { label: t("menu.placeWindow"), icon: "window", action: go("window") },
      { label: t("menu.placeHole"), icon: "hole", action: go("hole") },
      { label: t("menu.placeDormer"), icon: "roof", action: go("dormer") },
    ];
  }

  function partEntries(pi: number): MenuEntry[] {
    if (!project?.parts[pi]) return [];
    const ps = project.parts[pi].edges.map((e) => e.start);
    const w = Math.max(...ps.map((p) => p.x)) - Math.min(...ps.map((p) => p.x));
    return [
      { label: t("tool.transform") + "…", icon: "move", action: () => { selection = { part: pi, edge: selection?.part === pi ? selection.edge : 0 }; openTransform(); } },
      { label: t("menu.duplicatePart"), icon: "copy", action: () => {
        if (!project) return;
        // 2 m to the right of everything drawn (as a new standard roof is
        // placed), so the copy never lands on, and joins, another building
        // or a manual face (UX-010).
        const xs = [...project.parts.flatMap((p) => p.edges.map((e) => e.start.x)), ...(project.manual_faces ?? []).flatMap((f) => f.points.map((p) => p.x))];
        const dx = Math.max(w, Math.max(...xs) - Math.min(...ps.map((p) => p.x))) + 2;
        const added = transformPart(project, pi, translation(+dx.toFixed(3), 0), 1);
        if (added.length) selection = { part: added[0], edge: 0 };
        commit();
        setTimeout(() => plan?.fit(), 0);
      } },
      { label: t("tool.deletePart"), icon: "trash", danger: true, action: () => { selection = { part: pi, edge: 0 }; deletePart(); } },
    ];
  }

  function openMenu(target: PlanTarget, client: { x: number; y: number }, world: Point2) {
    if (!project) return;
    let title: string | undefined;
    let entries: MenuEntry[] = [];
    if (target.type === "item") {
      const it = target.item;
      if (!itemExists(project, it)) return;
      selectItem(it);
      title = itemTitle(it);
      // A window follows its roof face and a dormer faces down its roof: neither turns.
      const turns = it.kind !== "window" && it.kind !== "dormer";
      entries = [
        { label: t("menu.properties"), icon: "sliders", action: focusProperties },
        { label: t("menu.duplicate"), icon: "copy", hint: "Ctrl+D", action: () => duplicateItem(it) },
        ...(turns
          ? [
              { label: t("menu.rotateLeft"), icon: "undo", action: () => turnItem(it, 90) },
              { label: t("menu.rotateRight"), icon: "redo", action: () => turnItem(it, -90) },
            ]
          : []),
        ...(it.kind === "chimney" || it.kind === "hole" ? [{ label: t("menu.alignWall"), icon: "fit", action: () => alignItem(it) }] : []),
        { sep: true },
        { label: t("menu.delete"), icon: "trash", danger: true, hint: "Del", action: () => removeItem(it) },
      ];
    } else if (target.type === "edge") {
      const { part, edge } = target;
      const e = project.parts[part]?.edges[edge];
      if (!e) return;
      selection = { part, edge };
      title = t("def.wall", { n: edge + 1 });
      const gable = isGable(e);
      const marker = isMarker(e);
      const es = project.parts[part].edges;
      const a = e.start;
      const b = es[(edge + 1) % es.length].start;
      // The press projected onto the wall, so the new point lies on it.
      const len2 = (b.x - a.x) ** 2 + (b.y - a.y) ** 2 || 1;
      const u = Math.max(0.05, Math.min(0.95, ((world.x - a.x) * (b.x - a.x) + (world.y - a.y) * (b.y - a.y)) / len2));
      const on = { x: +(a.x + (b.x - a.x) * u).toFixed(3), y: +(a.y + (b.y - a.y) * u).toFixed(3) };
      entries = [
        { label: t("menu.properties"), icon: "sliders", action: focusProperties },
        ...(marker ? [] : [
          gable
            ? { label: t("menu.makeEave"), icon: "roof", action: () => patchEdge({ pitch_deg: presets.pitch_deg, half_hip: null }) }
            : { label: t("menu.makeGable"), icon: "house", action: () => patchEdge({ pitch_deg: GABLE_PITCH }) },
          { label: t("menu.insertPoint"), icon: "plus", action: () => insertVertex(part, edge, on.x, on.y) },
          { label: t("edge.addMarker"), icon: "right", action: insertMarker },
        ]),
        ...(!marker && !gable ? [{ label: t("calc.button"), icon: "chart", action: () => (calc = { part, edge, patches: null }) }] : []),
        { sep: true },
        ...partEntries(part),
      ];
    } else if (target.type === "vertex") {
      const { part, vertex } = target;
      if (!project.parts[part]?.edges[vertex]) return;
      selection = { part, edge: vertex };
      title = t("plan.vertex", { n: vertex + 1 });
      const es = project.parts[part].edges;
      const canDelete = isMarker(es[vertex]) || es.filter((x) => !isMarker(x)).length > 3;
      entries = [
        { label: t("menu.properties"), icon: "sliders", action: focusProperties },
        { label: t("menu.deletePoint"), icon: "trash", danger: true, hint: "Del", disabled: !canDelete, action: deleteVertex },
      ];
    } else if (target.type === "part") {
      const pi = target.part;
      if (!project.parts[pi]) return;
      title = partLabel(project.parts[pi].name, pi);
      entries = [...placeEntries(world), { sep: true }, ...partEntries(pi)];
    } else {
      const add = placeEntries(world);
      entries = [
        ...add,
        ...(add.length ? [{ sep: true } as MenuEntry] : []),
        { label: t("tool.draw"), icon: "draw", hint: "Insert", action: startDraw },
        { label: t("tool.standard"), icon: "house", action: () => (std = { part: null, mode: "add" }) },
        { label: t("plan.fit"), icon: "fit", action: () => plan?.fit() },
      ];
    }
    menu = { x: client.x, y: client.y, title, entries };
  }

  /** Arrow keys nudge the selected object by 5 cm (Shift: 50 cm). */
  function nudge(e: KeyboardEvent): boolean {
    const it = selectedItem;
    if (!project || !it || !itemExists(project, it)) return false;
    const d = e.shiftKey ? 0.5 : 0.05;
    const v = { ArrowLeft: [-d, 0], ArrowRight: [d, 0], ArrowUp: [0, d], ArrowDown: [0, -d] }[e.key];
    if (!v) return false;
    e.preventDefault();
    const from = itemCentre(project, it);
    const before = result;
    moveItem(project, it, v[0], v[1]);
    if (from) followRoof(project, it, from, { x: from.x + v[0], y: from.y + v[1] }, before);
    commit();
    return true;
  }

  async function togglePlacing(kind: "dormer" | "chimney" | "window" | "hole" | "line" | "face") {
    // Another tool ends the footprint being drawn: ask as Escape does (UX-013).
    if (drawing && !(await cancelDraw(true))) return;
    placing = placing === kind ? false : kind;
  }

  function setDormer(d: Dormer) {
    if (!project || dormerSel === null) return;
    project.dormers[dormerSel] = d;
    commit();
  }

  /** Asks first (UI-006): one press would remove the dormer with its faces. */
  async function deleteDormer() {
    if (!project || dormerSel === null) return;
    const index = dormerSel;
    const dormer = project.dormers[index];
    // Tauri's dialog plugin makes confirm asynchronous: always await it.
    if (!(await confirm(t("dormer.confirmDelete", { n: index + 1 })))) return;
    // The project may have been reloaded (AI edit) while the dialog was open.
    if (!project || project.dormers[index] !== dormer) return;
    project.dormers.splice(index, 1);
    if (dormerSel === index) dormerSel = null;
    commit();
  }

  /** Whether anything is drawn that Clear would remove. */
  function hasDrawing(p: Project | null): boolean {
    if (!p) return false;
    return [p.parts, p.dormers, p.chimneys, p.windows, p.roof_openings, p.manual_lines, p.manual_faces].some((l) => (l?.length ?? 0) > 0);
  }

  // Removes everything drawn and leaves an empty canvas (MCP clear_roof);
  // job data and roof options stay, and Undo brings the roof back.
  async function clearRoof() {
    if (!hasDrawing(project)) return;
    const before = project;
    if (!(await confirm(t("editor.confirmClear")))) return;
    // The project may have been reloaded (AI edit) while the dialog was open.
    if (!project || project !== before) return;
    project.parts = [];
    project.dormers = [];
    project.chimneys = [];
    project.windows = [];
    project.roof_openings = [];
    project.manual_lines = [];
    project.manual_faces = [];
    selection = null;
    dormerSel = null;
    openSel = null;
    manualSel = null;
    placing = false;
    commit();
  }

  async function deletePart() {
    if (!project || !selection) return;
    // The last part takes the whole roof with it: that is Clear.
    if (project.parts.length === 1) return clearRoof();
    const index = selection.part;
    const part = project.parts[index];
    // Tauri's dialog plugin replaces window.confirm with an async version
    // (a Promise is always truthy), so the answer must be awaited.
    if (!(await confirm(t("editor.confirmDeletePart", { name: partLabel(part.name, index) })))) return;
    // The project may have been reloaded (AI edit) while the dialog was open.
    if (!project || project.parts[index] !== part) return;
    project.parts.splice(index, 1);
    selection = null;
    commit();
  }

  // Drawing a new footprint.
  function startDraw() {
    // Already drawing: the Insert key keeps going; the toolbar button
    // (toggle) and the context menu go through cancelDraw, which asks.
    if (drawing) return;
    selection = null;
    dormerSel = null;
    openSel = null;
    placing = false;
    drawing = [];
  }

  /**
   * Stops drawing the footprint. `ask` (a key or a second press of the
   * Building button, easy to hit by accident) asks first once two or more
   * corners would be lost; the bar's Cancel button stops at once.
   */
  /** Resolves to whether no footprint is being drawn any more. */
  async function cancelDraw(ask: boolean): Promise<boolean> {
    const pts = drawing;
    if (!pts) return true;
    // Tauri's dialog plugin makes confirm asynchronous: always await it.
    if (ask && pts.length >= 2 && !(await confirm(t("draw.confirmCancel", { n: pts.length })))) return false;
    // Closed or restarted while the question was open.
    if (drawing !== pts) return false;
    drawing = null;
    return true;
  }

  function toggleDraw() {
    if (drawing) cancelDraw(true);
    else startDraw();
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

  function rename(v: string) {
    if (!project || v === project.name) return;
    project.name = v;
    commit();
  }

  // macOS keyboards have no Delete key (their "delete" sends Backspace) and
  // show shortcuts with ⌘.
  const isMac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
  const keys = (k: string) => (isMac ? k.replace("Ctrl+Shift+", "⇧⌘").replace("Ctrl+", "⌘") : k);

  function onkeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    const del = e.key === "Delete" || (e.key === "Backspace" && !mod);
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
    // A dialog is open: its keys (Escape closes it) must not edit the roof behind it.
    if (document.querySelector('[role="dialog"]')) return;
    if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      e.shiftKey ? redo() : undo();
    } else if (mod && e.key.toLowerCase() === "y") {
      e.preventDefault();
      redo();
    } else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (mod && e.key.toLowerCase() === "d" && selectedItem && !drawing) {
      e.preventDefault();
      duplicateItem(selectedItem);
    } else if (!mod && !drawing && !placing && e.key.startsWith("Arrow") && nudge(e)) {
      // handled
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
    } else if (del && dormerSel !== null && !drawing) {
      deleteDormer();
    } else if (del && openSel && !drawing) {
      deleteOpening();
    } else if (del && manualSel && !drawing) {
      deleteManual();
    } else if (del && selection && !drawing) {
      deleteVertexByKey();
    }
  }
</script>

<svelte:window {onkeydown} />
<ContextMenu bind:menu />

<div class="editor">
  <header>
    <button class="icon-btn" onclick={onback} title={t("editor.back")} aria-label={t("editor.back")}><Icon name="back" /></button>
    <div class="crumbs">
      <button class="crumb" onclick={onback}>{t("lib.title")}</button>
      <span>/</span>
      {#if project}
        <input class="title" type="text" use:textfield={{ value: project.name, required: true, onchange: rename }} aria-label={t("editor.name")} />
        {#if project.job.number}<span class="jobno">{project.job.number}</span>{/if}
      {/if}
    </div>
    <div class="spacer"></div>
    <button class="ai-pill" class:active={aiHere} onclick={() => { dock = "ai"; dockOpen = true; }} title={t("ai.title")}>
      <span class="dot"></span>
      {#if aiHere && live.status}
        <strong>{actorName(live.status.actor)}</strong><span class="msg">{liveText(live.status)}</span>
      {:else if isFresh() && live.status}
        <!-- The agent works on another job (or the library): said, not lit up. -->
        <strong>{actorName(live.status.actor)}</strong><span class="msg">{t("ai.elsewhere", { name: live.status.name ?? "" })}</span>
      {:else}
        <Icon name="sparkle" size={14} />{t("ai.short")}
      {/if}
    </button>
    <LanguagePicker />
  </header>

  {#if !loadError}
  <div class="toolbar" class:compact={compactTools} role="toolbar" bind:this={toolbarEl} use:hscroll>
    <div class="group">
      <button class="tool" onclick={undo} disabled={at <= 0} title="{t('editor.undo')} ({keys('Ctrl+Z')})"><Icon name="undo" /><span>{t("editor.undo")}</span></button>
      <button class="tool" onclick={redo} disabled={at >= historyLen - 1} title="{t('editor.redo')} ({keys('Ctrl+Shift+Z')})"><Icon name="redo" /><span>{t("editor.redo")}</span></button>
    </div>
    <div class="group">
      <button class="tool" class:on={!!std} aria-pressed={!!std} onclick={() => (std = { part: null, mode: project?.parts.length && selection ? "replace" : "add" })} title={t("tool.standardHint")}><Icon name="house" /><span>{t("tool.standard")}</span></button>
      <button class="tool" class:on={!!drawing} aria-pressed={!!drawing} onclick={toggleDraw} title="{t('tool.drawHint')} (Insert)"><Icon name="draw" /><span>{t("tool.draw")}</span></button>
      <button class="tool" class:on={placing === "dormer"} aria-pressed={placing === "dormer"} disabled={!project?.parts.length} onclick={() => togglePlacing("dormer")} title={t("tool.dormerHint")}><Icon name="roof" /><span>{t("tool.dormer")}</span></button>
      <button class="tool" class:on={placing === "chimney"} aria-pressed={placing === "chimney"} disabled={!project?.parts.length} onclick={() => togglePlacing("chimney")} title={t("tool.chimneyHint")}><Icon name="chimney" /><span>{t("tool.chimney")}</span></button>
      <button class="tool" class:on={placing === "window"} aria-pressed={placing === "window"} disabled={!project?.parts.length} onclick={() => togglePlacing("window")} title={t("tool.windowHint")}><Icon name="window" /><span>{t("tool.window")}</span></button>
      <button class="tool" class:on={placing === "hole"} aria-pressed={placing === "hole"} disabled={!project?.parts.length} onclick={() => togglePlacing("hole")} title={t("tool.holeHint")}><Icon name="hole" /><span>{t("tool.hole")}</span></button>
    </div>
    <div class="group">
      <button class="tool" class:on={placing === "line"} aria-pressed={placing === "line"} disabled={!result} onclick={() => togglePlacing("line")} title={t("tool.lineHint")}><Icon name="line" /><span>{t("tool.line")}</span></button>
      <button class="tool" class:on={placing === "face"} aria-pressed={placing === "face"} disabled={!result} onclick={() => togglePlacing("face")} title={t("tool.faceHint")}><Icon name="face" /><span>{t("tool.face")}</span></button>
      <button class="tool" class:on={!!project?.flat_roof} aria-pressed={!!project?.flat_roof} disabled={!project?.parts.length} onclick={toggleFlat} title={t("tool.flatHint")}><Icon name="flat" /><span>{t("tool.flat")}</span></button>
    </div>
    <div class="group">
      <button class="tool" class:on={!!xform} aria-pressed={!!xform} disabled={!selection} onclick={openTransform} title={t("tool.transformHint")}><Icon name="move" /><span>{t("tool.transform")}</span></button>
      <button class="tool" disabled={!selection} onclick={deletePart} title={t("tool.deletePartHint")}><Icon name="trash" /><span>{t("tool.deletePart")}</span></button>
      <button class="tool" disabled={!hasDrawing(project)} onclick={clearRoof} title={t("tool.clearHint")}><Icon name="eraser" /><span>{t("tool.clear")}</span></button>
    </div>
    <div class="group">
      <button class="tool" class:on={prefs.layout === "plan"} aria-pressed={prefs.layout === "plan"} onclick={() => setLayout("plan")} title={t("editor.plan")}><Icon name="plan" /><span>{t("editor.plan")}</span></button>
      <button class="tool" class:on={prefs.layout === "split"} aria-pressed={prefs.layout === "split"} onclick={() => setLayout("split")} title={t("editor.both")}><Icon name="split" /><span>{t("editor.both")}</span></button>
      <button class="tool" class:on={prefs.layout === "3d"} aria-pressed={prefs.layout === "3d"} onclick={() => setLayout("3d")} title="{t('editor.modelHint')} (F3)"><Icon name="cube" /><span>{t("editor.model")}</span></button>
      <button class="tool" onclick={() => plan?.fit()} disabled={prefs.layout === "3d"} title={t("plan.fitHint")}><Icon name="fit" /><span>{t("plan.fit")}</span></button>
      <button class="tool" class:on={showDims} aria-pressed={showDims} onclick={() => (showDims = !showDims)} title={t("tool.dims")}><Icon name="dims" /><span>{t("tool.dims")}</span></button>
    </div>
    <div class="group">
      <button class="tool" class:on={exporting} aria-pressed={exporting} disabled={!result || !!blockedReason} onclick={() => (exporting = true)} title={blockedReason ?? t("tool.exportHint")}><Icon name="download" /><span>{t("tool.export")}</span></button>
    </div>
  </div>
  {/if}

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
              <span class="review-summary" title={pendingList.map((a) => entrySummary(a.summary)).reverse().join(" → ")}>{pendingList.map((a) => entrySummary(a.summary)).reverse().join(" → ")}</span>
              {#if pendingChanges.length}
                <span class="review-list">
                  {pendingChanges.slice(0, 4).map((c) => describeChange(c, project)).join(" · ")}{pendingChanges.length > 4 ? ` · ${t("review.more", { n: pendingChanges.length - 4 })}` : ""}
                </span>
              {/if}
              {#if review?.differences?.length}
                <span class="review-figures">
                  {review.differences.slice(0, 3).map((d) => describeFigure(d, project)).join(" · ")}{review.differences.length > 3 ? ` · ${t("review.more", { n: review.differences.length - 3 })}` : ""}
                </span>
              {/if}
            </div>
            {#if review?.before}
              <button class="btn sm" class:on={showBefore} aria-pressed={showBefore} onclick={() => (showBefore = !showBefore)} title={t("review.beforeHint")}>{t("review.before")}</button>
            {/if}
            <button class="btn sm" disabled={reviewBusy} onclick={() => revert(true)}><Icon name="undo" size={14} />{pendingList.length > 1 ? t("review.revertAll") : t("ai.revert")}</button>
            <button class="btn sm accept" disabled={reviewBusy} onclick={accept}><Icon name="check" size={14} />{t("review.accept")}</button>
          </div>
        {/if}
        {#if calcError}
          {@const d = describeCalcError(calcError)}
          <div class="warn-bar" role="alert">
            <i class="dot"></i>
            <div class="warn-text">
              <strong>{d.title}</strong>
              <span>{d.hint}</span>
            </div>
            <code>{d.code}</code>
            {#if at > 0}<button class="btn sm" onclick={undo}><Icon name="undo" size={14} />{t("editor.undo")}</button>{/if}
          </div>
        {/if}
        {#if result?.problems?.length}
          {@const first = result.problems[0]}
          <div class="warn-bar" role="alert">
            <i class="dot"></i>
            <div class="warn-text">
              <strong>{t("prob.title", { n: result.problems.length })}</strong>
              <span>{describeProblem(first, project)}{#if result.problems.length > 1} · {t("review.more", { n: result.problems.length - 1 })}{/if}</span>
            </div>
            <code>{first.code}</code>
            {#if first.item === "part" && first.edge !== undefined}
              <button class="btn sm" onclick={() => (selection = { part: first.index, edge: first.edge! })}>{t("opening.show")}</button>
            {/if}
          </div>
        {/if}
        {#if nonConvex.length}
          <div class="warn-bar" role="alert">
            <i class="dot"></i>
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
                {highlightItems}
                {ghost}
                mark={dock === "breakdown" && dockOpen ? planMark : null}
                {faceLabels}
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
                onmoveend={endVertexDrag}
                oninsert={insertVertex}
                ondrawpoint={(x, y) => drawing?.push({ x, y })}
                ondrawclose={finishDraw}
                oncursor={(p) => (cursor = p)}
                ondrag={dragObject}
                oncontext={openMenu}
              />
            </div>
          {/if}
          {#if prefs.layout !== "plan"}
            <div class="view">
              <span class="view-label">{t("editor.model")}</span>
              <Roof3D {project} {result} {selection} item={selectedItem}
                onpick={(tg) => { if (tg.type === "item") selectItem(tg.item); else if (tg.type === "edge") { selectItem(null); selection = { part: tg.part, edge: tg.edge }; } }}
                oncontext={openMenu} />
            </div>
          {/if}
        </div>
        {#if drawing}
          <DrawBar
            count={drawing.length}
            onstep={drawStep}
            onundo={() => drawing?.pop()}
            onclose={finishDraw}
            oncancel={(ask) => cancelDraw(!!ask)}
          />
        {:else if dormerSel !== null}
          <DormerBar {project} {result} bind:index={dormerSel} onchange={setDormer} ondelete={deleteDormer} onedit={editDormer} />
        {:else if openSel}
          <OpeningBar {project} bind:sel={openSel} onchimney={setChimney} onwindow={setWindow} ondelete={deleteOpening} />
        {:else if manualSel}
          <ManualBar {project} bind:sel={manualSel} online={setManualLine} onface={setManualFace} ontopart={faceToPart} ondelete={deleteManual} />
        {:else}
          <RoofDefBar
            {project}
            bind:selection
            onchange={patchEdge}
            onapplyall={applyAll}
            onheight={setPartHeight}
            onlength={setLength}
            onpoint={(x, y) => { if (selection) { moveVertex(selection.part, selection.edge, x, y); commit(); } }}
            ondelete={deleteVertex}
            onmarker={insertMarker}
            onround={opposite !== null ? openRound : undefined}
            oncalc={() => selection && (calc = { part: selection.part, edge: selection.edge, patches: null })}
          />
        {/if}
      </div>

      <aside class:open={dockOpen}>
        <div class="tabs" role="tablist" use:tablist>
          <button role="tab" id={dockIds.tab("results")} aria-controls={dockIds.panel} aria-selected={dock === "results"} class:on={dock === "results"} onclick={() => showDock("results")} title={t("dock.results")}><Icon name="chart" size={15} /><span class="tab-label">{t("dock.results")}</span></button>
          <button role="tab" id={dockIds.tab("breakdown")} aria-controls={dockIds.panel} aria-selected={dock === "breakdown"} class:on={dock === "breakdown"} onclick={() => showDock("breakdown")} title={t("bd.title")}><Icon name="sigma" size={15} /><span class="tab-label">{t("dock.breakdown")}</span></button>
          <button role="tab" id={dockIds.tab("ai")} aria-controls={dockIds.panel} aria-selected={dock === "ai"} class:on={dock === "ai"} onclick={() => showDock("ai")} title={t("dock.ai")}>
            <Icon name="sparkle" size={15} /><span class="tab-label">{t("dock.ai")}</span>{#if pending}<i class="badge"></i>{/if}
          </button>
          <button role="tab" id={dockIds.tab("job")} aria-controls={dockIds.panel} aria-selected={dock === "job"} class:on={dock === "job"} onclick={() => showDock("job")} title={t("dock.job")}><Icon name="briefcase" size={15} /><span class="tab-label">{t("dock.job")}</span></button>
        </div>
        <div class="dock" role="tabpanel" id={dockIds.panel} aria-labelledby={dockIds.tab(dock)} bind:this={dockEl}>
          <button class="icon-btn dock-close" onclick={() => (dockOpen = false)} aria-label={t("common.close")} title={t("common.close")}><Icon name="x" size={16} /></button>
          {#if dock === "results"}
            <Results {result} {project} onstore={storeResult} blocked={blockedReason} />
          {:else if dock === "breakdown"}
            <Breakdown {project} {result} bind:data={breakdown} onmark={(mk) => (planMark = mk)} />
          {:else if dock === "ai"}
            <AiPanel {id} {project} {activity} onrevert={() => revert(false)} busy={reviewBusy} {onconnect} onfocus={(c) => (focusChange = c)} />
          {:else}
            <JobPanel {id} {project} onchange={commit} onsave={save} blocked={blockedReason} track={(p) => pendingEdits.track(p)} />
          {/if}
        </div>
      </aside>
    </div>

    <footer class="statusbar">
      <span class="coords">{cursor ? `X ${num(cursor.x)}   Y ${num(cursor.y)}` : "X —   Y —"}</span>
      <span class="opt">{t("status.snap")}</span>
      <span class="opt">{t("status.walls", { n: wallCount, p: project.parts.length })}</span>
      {#if result}<span>{t("res.area")}: <b>{m2(result.totals.roof_area)}</b></span>{/if}
      <span class="spacer"></span>
      <span class="save">
        {#if saveState === "saved"}<Icon name="check" size={13} />{t("editor.saved")}{:else if saveState === "saving"}{t("editor.saving")}{:else if saveState === "failed"}<span class="save-failed"><i class="dot"></i>{t("editor.saveFailed")}</span><button class="retry" onclick={save}>{t("err.retry")}</button>{:else}{t("editor.dirty")}{/if}
      </span>
    </footer>
  {:else if loadError}
    <div class="load-error" role="alert">
      <code class="code">{loadErrorCode}</code>
      <h2>{loadError.title}</h2>
      <p>{loadError.message}</p>
      {#if loadError.detail}<pre>{loadError.detail}</pre>{/if}
      <div class="load-actions">
        <button class="btn" onclick={onback}><Icon name="back" size={16} />{t("editor.back")}</button>
        {#if loadErrorCode !== "not_found" && loadErrorCode !== "unreadable" && loadErrorCode !== "newer_format"}
          <button class="btn primary" onclick={() => { loadError = null; load(true); }}><Icon name="refresh" size={16} />{t("err.retry")}</button>
        {/if}
      </div>
    </div>
  {/if}
</div>

{#if exporting && project}
  <ExportDialog {project} title={exportTitle(project, id)} onclose={() => (exporting = false)} />
{/if}

{#if dormerDlg && project}
  <Modal title={dormerDlg.index === null ? t("dormer.dialogNew") : t("dormer.n", { n: dormerDlg.index + 1 })} width={760} onclose={() => (dormerDlg = null)}>
    <DormerDialog {project} bind:params={dormerDlg.params} />
    {#snippet footer()}
      <button class="btn" onclick={() => (dormerDlg = null)}>{t("common.cancel")}</button>
      <button class="btn primary" onclick={applyDormerDialog}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

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

{#if xform && project}
  <Modal title={t("xf.title")} width={560} onclose={() => (xform = null)}>
    <TransformForm part={project.parts[xform.part]} edge={xform.edge} bind:mode={xform.mode} bind:op={xform.op} />
    {#snippet footer()}
      <button class="btn" onclick={() => (xform = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={!xform?.op} onclick={applyTransform}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

{#if std}
  <Modal title={t("std.title")} width={640} onclose={() => (std = null)}>
    <!-- A preview still being calculated may come back after the dialog closed. -->
    <StandardRoofForm bind:part={() => std?.part ?? null, (v) => std && (std.part = v)} />
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
    width: clamp(110px, 24vw, 280px);
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
    max-width: min(420px, 32vw);
    min-width: 0;
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
    position: relative;
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
    padding: 9px 14px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    color: var(--text);
    font-size: 13px;
  }
  .warn-bar > span {
    flex: 1;
  }
  .warn-bar .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 1px;
    background: var(--danger);
  }
  .warn-bar code,
  .load-error .code {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11px;
    color: var(--text-3);
  }
  .warn-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    column-gap: 10px;
    row-gap: 2px;
  }
  .warn-text strong {
    font-weight: 600;
  }
  .warn-text span {
    color: var(--text-2);
  }
  .save-failed .dot {
    width: 6px;
    height: 6px;
    border-radius: 1px;
    background: var(--danger);
  }
  .save-failed {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--danger);
    font-weight: 600;
  }
  .retry {
    margin-left: 6px;
    padding: 0 8px;
    height: 20px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: var(--surface);
    font: inherit;
    cursor: pointer;
  }
  .retry:hover {
    background: var(--surface-2);
  }
  .load-error {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 8px;
    width: min(560px, 100% - 48px);
    margin: 0 auto;
  }
  .load-error h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }
  .load-error p {
    margin: 0;
    color: var(--text-2);
    line-height: 1.5;
  }
  .load-error pre {
    align-self: stretch;
    margin: 4px 0 0;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text-2);
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .load-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
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
  /* Two lines at most: a long AI run must not squeeze the plan away (UI-001). */
  .review-summary {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }
  .review-list {
    color: var(--text-2);
    font-size: 12.5px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .review-figures {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 12px;
    color: var(--text-2);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .btn.sm {
    height: 30px;
    padding: 0 12px;
    font-size: 13px;
  }
  .btn.sm.on {
    border-color: color-mix(in srgb, var(--ai) 60%, var(--border));
    color: var(--ai-strong);
  }
  .btn.accept {
    background: var(--ai);
    border-color: var(--ai);
    color: var(--on-ai);
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
    position: relative;
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
    flex: 1 1 auto;
    /* Longer labels (de "Ergebnisse", "Rechenweg") shrink instead of
       pushing the dock out of the window (UI-013). */
    min-width: 0;
    padding: 0 3px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 36px;
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-2);
  }
  .tab-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .dock-close {
    display: none;
  }
  .statusbar {
    display: flex;
    align-items: center;
    gap: 18px;
    overflow: hidden;
    white-space: nowrap;
    height: 26px;
    padding: 0 12px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-2);
  }
  .coords {
    min-width: 130px;
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

  /* Edit bars below the views never take more than about half of a low window. */
  .main > :global(.bar) {
    flex: none;
    max-height: 46vh;
    overflow-y: auto;
  }

  /* Labels that do not fit the window (long languages): icons only, as below. */
  .toolbar.compact .tool {
    width: 36px;
    height: 36px;
    min-width: 0;
    padding: 0;
  }
  .toolbar.compact .tool span {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .toolbar.compact .group {
    padding-right: 4px;
    margin-right: 2px;
  }

  /* ---- Responsive: half-screen and smaller windows ---- */
  @media (max-width: 1440px) {
    /* Narrow enough that the labelled toolbar fits 1360 px in Czech. */
    .tool {
      min-width: 0;
      padding: 0 5px;
    }
  }
  /* Narrow or low window: the toolbar collapses to icons (tooltips keep the names). */
  @media (max-width: 1220px), (max-height: 640px) {
    .tool {
      width: 36px;
      height: 36px;
      padding: 0;
    }
    .tool span {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
    }
    .group {
      padding-right: 4px;
      margin-right: 2px;
    }
  }
  @media (max-width: 1100px) {
    aside {
      width: 290px;
    }
    .statusbar {
      gap: 12px;
    }
  }
  /* Half-screen: the side panel becomes a rail of tabs that opens over the views. */
  @media (max-width: 900px) {
    header {
      gap: 4px;
      padding: 6px 8px;
    }
    .toolbar {
      flex-wrap: wrap;
      overflow: visible;
      row-gap: 2px;
    }
    aside {
      width: 44px;
    }
    .tabs {
      flex: 1;
      flex-direction: column;
      justify-content: flex-start;
      padding: 6px 4px;
      border-bottom: none;
    }
    .tabs button {
      flex: none;
      height: 36px;
      border-bottom: none;
      border-radius: 8px;
    }
    .tabs button .tab-label {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
    }
    .tabs button.on {
      background: var(--surface-2);
    }
    aside.open .tabs button.on {
      background: var(--accent-soft);
      color: var(--accent);
    }
    .badge {
      top: 5px;
      right: 5px;
    }
    .dock {
      display: none;
    }
    aside.open .dock {
      display: block;
      position: absolute;
      z-index: 20;
      top: 0;
      right: 100%;
      bottom: 0;
      width: min(340px, calc(100vw - 60px));
      padding-top: 40px;
      background: var(--surface);
      border-left: 1px solid var(--border);
      box-shadow: var(--shadow-lg);
    }
    .dock-close {
      display: inline-grid;
      position: absolute;
      top: 6px;
      right: 8px;
    }
    .opt {
      display: none;
    }
  }
  @media (max-width: 700px) {
    .crumb,
    .crumbs > span:not(.jobno) {
      display: none;
    }
  }
  /* Taller than wide (e.g. left/right half of a screen): plan above 3D instead of side by side. */
  @media (orientation: portrait) {
    .views.split {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr 1fr;
    }
  }
  /* Keep the plan hint to one line so it does not cover the drawing in smaller views. */
  .view :global(.hint) {
    max-width: calc(100% - 20px);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  /* Very low window: keep the views usable; toolbar scrolls instead of wrapping. */
  @media (max-height: 520px) {
    header {
      padding-top: 3px;
      padding-bottom: 3px;
    }
    .toolbar {
      flex-wrap: nowrap;
      overflow-x: auto;
      padding-top: 2px;
      padding-bottom: 2px;
    }
    .main > :global(.bar) {
      max-height: 36vh;
    }
  }
  @media (max-height: 640px) {
    .review,
    .warn-bar {
      padding: 6px 12px;
    }
  }
</style>
