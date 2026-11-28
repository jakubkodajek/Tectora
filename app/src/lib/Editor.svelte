<script lang="ts">
  // Design editor laid out like DACAD (toolbar, plan + 3D windows, roof
  // definition bar, status bar) with a side dock for results, AI activity
  // and job data.
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { maxOf, minOf } from "./extent";
  import AiPanel from "./AiPanel.svelte";
  import DrawBar from "./DrawBar.svelte";
  import PlaceBar from "./PlaceBar.svelte";
  import { lastStep } from "./drawstep";
  import { stepWall, wallStepKey } from "./wallstep";
  import { acceptTs, reviewStatusKey, type ReviewPress } from "./reviewstatus";
  import { bannerChanges, beforeKey as reviewBeforeKey } from "./reviewbanner";
  import { deleteKeyHint, drawKeyHint, editorTakesUndo, isDrawKey, isMacPlatform, shortcut, undoKey, zoomKey, draftKey } from "./keys";
  import Icon from "./Icon.svelte";
  import JobPanel from "./JobPanel.svelte";
  import LanguagePicker from "./LanguagePicker.svelte";
  import Modal from "./Modal.svelte";
  import { openModals } from "./modalcount";
  import ExportDialog from "./ExportDialog.svelte";
  import PlanView from "./PlanView.svelte";
  import Results from "./Results.svelte";
  import Breakdown from "./Breakdown.svelte";
  import RoofDefBar from "./RoofDefBar.svelte";
  import DormerBar from "./DormerBar.svelte";
  import { hasBuilding } from "./offer";
  import DormerDialog from "./DormerDialog.svelte";
  import OpeningBar from "./OpeningBar.svelte";
  import ManualBar from "./ManualBar.svelte";
  import { commitFocused } from "./commitfocus";
  import { textfield } from "./textfield";
  import ContextMenu, { type MenuEntry, type MenuState } from "./ContextMenu.svelte";
  import { defaultDirection, moveCorner, samePoint } from "./corners";
  import { MCP_KIND, followRoof, itemCentre, itemExists, itemOf, moveItem, rotateItem, deleteItem, placementOf, transformItem, type ItemRef, type PlanTarget } from "./items";
  import { snapToRoof, type ManualSel } from "./manual";
  import { keptSelection } from "./selection";
  import { fitlabels } from "./fitlabels";
  import { describeCalcError, describeError, describeProblem, toAppError, type AppError, type Described } from "./errors";
  import { dismiss, info, notify, notifyError, notifyRefusal, notifyWarning, toasts } from "./notify.svelte";
  import { nonConvexOpenings, openingList, type OpeningSel } from "./openings";
  import { roundTo } from "./rounding";
  import { buildDormer, defaultParams, nearestWallAngle, parseDormer, roofHeightAt, type DormerParams } from "./dormers";
  import Roof3D from "./Roof3D.svelte";
  import StandardRoofForm from "./StandardRoofForm.svelte";
  import RoundRoofForm from "./RoundRoofForm.svelte";
  import RoofCalcForm from "./RoofCalcForm.svelte";
  import TransformForm from "./TransformForm.svelte";
  import { transformPart, translation, type TransformMode, type TransformOp } from "./transform";
  import { latest, sequence } from "./latest";
  import { api, onProjectsChanged } from "./api";
  import { handOverDeleted } from "./deleted";
  import { changedEdges, changedItems, describeChange, describeFigure, ghostOf } from "./changes";
  import { wallNotes } from "./wallnotes";
  import { merge3, sameJob } from "./merge";
  import { keepMissedNotice, takeMissedNotice } from "./missed";
  import { JobEdits, heldName, notMade, type Hold, type ListName, type Outcome } from "./jobedits";
  import { hasRoofWarning, offRoofWarning, roofState, warningText } from "./results";
  import LineInfo from "./LineInfo.svelte";
  import { detailOf, sameLine } from "./lineinfo";
  import { isTyping, releaseLeaveGuard, setLeaveGuard } from "./leave";
  import { PendingEdits, exclusive } from "./inflight";
  import { CoreAction, DragSession, closedForInput, dropForeign, inertBut, refuseInput, type Drag, type DragState } from "./dragsession";
  import { m2, num } from "./format";
  import { i18n, t } from "./i18n.svelte";
  import { hscroll } from "./hscroll";
  import { partLabel } from "./partname";
  import { exportTitle } from "./exportlist";
  import { tabIds, tablist } from "./tabs";
  import { actorName, entryText, isFresh, live, liveText, pillText, receivedRecalculation, refreshedNotice } from "./live.svelte";
  import { prefs, presets, reviewedAt, savePrefs } from "./settings.svelte";
  import type { ActivityItem, Arc, HelperRequest, Recalculation, RefreshedEntry, ReviewStatus, CalcBreakdown, CalcResult, Change, Chimney, Dormer, EdgeDef, ManualFace, ManualLine, LineRef, PlanLabel, PlanMark, Point2, Project, RoofPart, RoofWindow, Selection } from "./types";
  import { GABLE_PITCH, buildingHeight, emptyProject, isAi, isGable, isMarker } from "./types";
  import { eavePitchFor, pruneEavePitches, rememberEavePitch, useEavePitchJob } from "./walltype";
  import { pressKeepingFocus } from "./press";

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
  // A pitch remembered for Gable then Eave belongs to this job only (UI-104).
  useEavePitchJob(id);

  let project = $state<Project | null>(null);
  let result = $state<CalcResult | null>(null);
  /** The result holds a warning that is not a notice: the Results tab carries the mark (UX-1401). */
  const roofWarned = $derived(hasRoofWarning(result));
  /** Why the last calculation of the whole job failed, for Results to keep saying it (UX-101). */
  let calcFailure = $state<Described | null>(null);
  let selection = $state<Selection>(null);
  /** The roof line under the pointer and the view it is in (its details are shown). */
  let lineHover = $state<(LineRef & { view: "plan" | "3d" }) | null>(null);
  /** The roof line clicked last: its details stay until Escape, × or a click elsewhere. */
  let linePin = $state<(LineRef & { view: "plan" | "3d"; a: number[]; b: number[] }) | null>(null);
  /** The line whose details are shown: the one under the pointer, else the pinned one. */
  const lineShown = $derived(lineHover ?? linePin);
  const lineDetail = $derived(detailOf(result, lineShown));
  const lineMarks = $derived([lineHover, linePin].filter((l): l is NonNullable<typeof l> => !!l));
  function pinLine(l: LineRef, view: "plan" | "3d") {
    const d = detailOf(result, l);
    if (!d) return;
    linePin = sameLine(linePin, l) ? null : { ...l, view, a: d.a, b: d.b };
  }
  // A new result renumbers the lines: keep the pinned one where it still is
  // (same end points within 1 mm), else let it go.
  $effect(() => {
    const r = result;
    const pin = untrack(() => linePin);
    if (!pin) return;
    const near = (p: number[], q: number[]) => p.every((v, k) => Math.abs(v - q[k]) < 1e-3);
    const d = r?.line_details?.find((x) => (near(x.a, pin.a) && near(x.b, pin.b)) || (near(x.a, pin.b) && near(x.b, pin.a)));
    linePin = d ? { ...pin, part: d.part, index: d.index } : null;
    lineHover = null;
  });
  let saveState = $state<"saved" | "saving" | "dirty" | "failed">("saved");
  /** Why the project could not be opened (shown instead of the editor). */
  let loadError = $state<Described | null>(null);
  let loadErrorCode = $state<string>("");
  let dock = $state<"results" | "breakdown" | "ai" | "job">("results");
  const dockIds = tabIds("dock");

  // The toolbar gives up its labels step by step when the labelled buttons do
  // not fit the window (German at 1360 or 1440 px, UX-006): first the labels
  // get smaller and may take two lines, then the view and export buttons drop
  // theirs, and only then every button shows its icon alone (with tooltips).
  // It takes each step back once the window is as wide as that step needed.
  let toolbarEl: HTMLDivElement | undefined = $state();
  /** 0 full labels, 1 small two-line labels, 2 view and export buttons icon only, 3 all icon only. */
  let toolLevel = $state(0);
  const compactTools = $derived(toolLevel >= 3);
  /** The width the toolbar needed at each level before it took the next one. */
  let levelWidths: number[] = [];
  function fitToolbar() {
    const el = toolbarEl;
    if (!el) return;
    if (el.scrollWidth > el.clientWidth + 1) {
      if (toolLevel >= 3) return;
      levelWidths[toolLevel] = el.scrollWidth;
      toolLevel += 1;
      tick().then(fitToolbar);
    } else if (toolLevel > 0 && el.clientWidth >= (levelWidths[toolLevel - 1] ?? Infinity)) {
      toolLevel -= 1;
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
      toolLevel = 0;
      levelWidths = [];
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
  /** The side panel is a drawer over the views (the window's CSS breakpoint, `max-width: 900px`). */
  const narrowDock = () => typeof window !== "undefined" && window.matchMedia("(max-width: 900px)").matches;

  function showDock(d: typeof dock) {
    dockOpen = dock !== d || !dockOpen;
    dock = d;
  }
  let drawing = $state<Point2[] | null>(null);
  let dormerSel = $state<number | null>(null);
  /** The dormer dialog: a new dormer (index null) or dormer `index`. */
  let dormerDlg = $state<{ index: number | null; params: DormerParams } | null>(null);
  /** Why the dormer dialog's OK cannot store the dormer (the dialog says it), else null. */
  let dormerBlocked = $state<string | null>(null);
  /** The open dormer dialog, for OK's last check (UI-203). */
  let dormerDialog = $state<ReturnType<typeof DormerDialog> | undefined>();
  /** A warning of the dormer dialog OK does not wait for, repeated next to OK. */
  let dormerNotice = $state<string | null>(null);
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
  /** Small windows show the review banner on one line; this opens its details (UX-017). */
  let reviewDetails = $state(false);
  let focusChange = $state<Change | null>(null);
  let std = $state<{ part: RoofPart | null; mode: "add" | "replace" } | null>(null);
  let plan: PlanView | undefined = $state();
  // Round-roof helper on the selected wall and the one facing it.
  let round = $state<{ part: number; a: number; b: number; span: number; eave: number; overhang: number; arc: Arc | null; request: HelperRequest | null } | null>(null);
  // The wall facing the selected one, from the core (`roof_helper_context`):
  // the round-roof button shows only where there is one. Asked again, after a
  // pause, whenever the selected part's walls change.
  let wallCtx = $state<{ part: number; edge: number; opposite: number | null } | null>(null);
  const askWall = latest(
    (part: number, edge: number, walls: Project["parts"][number]) => {
      const stub = emptyProject("wall");
      stub.parts = [walls];
      return api.roofHelperContext(stub, 0, edge).then((c) => ({ part, edge, opposite: c.opposite }));
    },
    (c) => (wallCtx = c),
    (e) => {
      wallCtx = null;
      notifyError(e, "calc");
    },
  );
  $effect(() => {
    const sel = selection;
    const part = sel ? project?.parts[sel.part] : undefined;
    if (!sel || !part || isMarker(part.edges[sel.edge])) {
      askWall.stop();
      wallCtx = null;
      return;
    }
    askWall.ask(sel.part, sel.edge, $state.snapshot(part) as RoofPart);
  });
  const opposite = $derived(wallCtx && selection && wallCtx.part === selection.part && wallCtx.edge === selection.edge ? wallCtx.opposite : null);

  // Undo history of serialized projects; `cursor` points at the current one.
  let history: string[] = [];
  let at = $state(0);
  let historyLen = $state(0);
  /**
   * Steps whose Undo or Redo goes through the core, so that whatever puts
   * AI work back into the job or takes it out again is a logged revert the
   * review reads (audit 2026-11-24 UX-1601, design A2 / A3). `undoVia[i]`:
   * leaving step `i` backwards takes back the revert logged at `take` (a
   * Revert in the review), or reverts every change from the AI's entry
   * `from` on (the AI's outside save). `redoVia[i]`: leaving step `i`
   * forwards takes back the revert logged at that time (the Undo's own).
   */
  type CoreUndo = { take: number } | { from: number };
  let undoVia: (CoreUndo | undefined)[] = [];
  let redoVia: (number | undefined)[] = [];
  /**
   * The newest change-log entry whose effect is in the editor's job: every
   * Undo / Redo through the core names it, and the core refuses with
   * `revert_stale` when the log has a newer one (an AI saved during the
   * press), so a key press never reverts a change the user has not seen
   * (audit 2026-11-24 ERR-1704, ARCH-1703, design B). Set only where a job
   * is applied: `load` when it applies the job read or finds it is the one
   * the editor has, and `applyCoreAnswer`; never by a bare read of the
   * history (ERR-1702). `null` until the first read.
   */
  let seenTs: number | null = null;
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
  /**
   * Another program made the open job a file Tectora cannot read
   * (`unreadable`, `too_many`): the next save writes the version open here
   * over it, which the user is told when it happens (audit 2026-11-23
   * ERR-1401). Cleared by that save and by a later good read. Meanwhile
   * its notice stays and the status bar does not say saved (UX-1504).
   */
  let outsideUnreadable = $state(false);
  /** The notice of `outsideUnreadable`, sticky until it is over. */
  let outsideNotice: number | null = null;
  /**
   * The time of `outsideUnreadable` is over: its notice goes, and when the
   * store says a save or a revert wrote over the file it could not read
   * (`repaired`), the user is told the other program's file was replaced
   * (audit 2026-11-23 ERR-1502: told from the store's answer, also when
   * the reload had not failed first).
   */
  function outsideOver(replaced: boolean) {
    outsideUnreadable = false;
    if (outsideNotice !== null) dismiss(outsideNotice);
    outsideNotice = null;
    // Not one of the editor's own notices: it tells what happened to the
    // other program's file and stays its time over the library too, also
    // when the save before Back or the editor's last save wrote it; so it
    // names the job (audit 2026-11-23 ERR-1602).
    if (replaced) info(t("err.ofJob", { name: project?.name.trim() || id, title: t("editor.outsideReplaced") }));
  }
  /**
   * A newer Tectora wrote the open job meanwhile: nothing made here can be
   * saved over it (the store refuses, the file is never written over). A
   * notice of its own that stays, and the status bar says so instead of
   * saved (audit 2026-11-23 UI-1603, HUNT-1604). Cleared by a good read or
   * a save that went through.
   */
  let outsideNewer = $state(false);
  let outsideNewerNotice: number | null = null;
  function newerOver() {
    outsideNewer = false;
    if (outsideNewerNotice !== null) dismiss(outsideNewerNotice);
    outsideNewerNotice = null;
  }
  /** Saves started so far: a read that a save overtook is stale. */
  let saveGen = 0;
  /** The last save failed: its code and its error notice (UX-1702). */
  let failedSave: { code: string; toast: number } | null = null;
  /**
   * The file is readable again after a save failed because of it (a newer
   * Tectora's file, one Tectora cannot read): that save's advice is stale,
   * so its notice goes and the edits are saved once more (UX-1702).
   */
  function retryFailedSave() {
    if (!failedSave || !["newer_format", "unreadable", "too_many"].includes(failedSave.code)) return;
    dismiss(failedSave.toast);
    failedSave = null;
    if (saveState === "failed") void save();
  }
  /** Saves run one after another: two from the same base would conflict
      with each other, and the merge would drop the later local value. */
  let saveQueue: Promise<unknown> = Promise.resolve();
  /** Outside saves merged in a row before a save gives up (`save_conflict`). */
  const SAVE_TRIES = 5;
  let calcSeq = 0;
  /** Field handlers that ask the core before they change the job: leaving waits for them (ERR-004). */
  /** Edits with the core (`pendingEdits`): the status bar says "saving" meanwhile, the file does not have them yet (UX-502). */
  let editsWaiting = $state(0);
  const pendingEdits = new PendingEdits((n) => (editsWaiting = n));

  const snapshot = () => JSON.stringify(project);

  /**
   * The one rule for edits that wait for the core (jobedits.ts): each holds
   * the item it was made for, they run one after another, an outside save
   * refuses only the edit whose own item it changed (said once, here), and
   * the user's own steps never refuse each other.
   */
  const jobEdits = new JobEdits(() => project, snapshot, (o, hold) => notMadeNote(o, hold));

  /**
   * Says that an edit was not made, naming the object it was for, as a
   * warning that stays long enough to be read (UX-401, UX-404): the typed
   * value is gone, the user must know which object to look at.
   */
  function notMadeNote(o: "gone" | "restless", hold: Hold | null) {
    notify({
      kind: "warning",
      title: hold ? t("editor.notMadeOf", { name: heldName(hold, project) }) : t("editor.notMade"),
      message: t(o === "gone" ? "editor.actionStale" : "editor.actionBusy"),
    });
  }
  /** An edit that reached the job while Revert or the put-back was out was dropped (`CoreAction.foreign`): said once, neutrally. */
  function refusedDuringAction() {
    notify({ kind: "warning", title: t("editor.notMade"), message: t("editor.refusedDuringAction") });
  }
  const plain = (p: Project) => $state.snapshot(p) as Project;
  /** One-shot actions under way: their buttons are off and a second press does nothing (UI-302, UI-308). */
  let acting = $state({ transform: false, calc: false, round: false, face: false, store: false, dormer: false, copy: false });
  /** What the open dialogs are for (kept out of the reactive state: the holds are moved by `edits`). */
  const dialogHolds: { calc?: Hold; xform?: Hold; round?: Hold; dormer?: Hold } = {};
  const SEL_LIST: Record<NonNullable<OpeningSel>["kind"], ListName> = { chimney: "chimneys", window: "windows", hole: "roof_openings" };
  const ITEM_LIST: Record<ItemRef["kind"], ListName> = { dormer: "dormers", chimney: "chimneys", window: "windows", hole: "roof_openings", line: "manual_lines", face: "manual_faces" };

  /**
   * For the bars: a core-backed edit of item `index` of `list`, held now and
   * queued behind the edits before it. Resolves to the reason when it was
   * not made (the field shows the stored value again); a refusal `ask`
   * throws reaches the bar. Leaving the job waits for it (ERR-004).
   */
  function barEdit<T>(list: ListName, index: number, ask: (job: Project, at: number) => Promise<T>, apply: (answer: T, at: number) => void): Promise<string | undefined> {
    return pendingEdits.track(jobEdits.run(jobEdits.hold(list, index), ask, apply).then(notMade));
  }

  /**
   * Swaps in `next`, the job after an outside save (an AI save, a merge on
   * a save conflict, Revert): `base` is the file as last read or written,
   * `remote` the file now. Edits under way, open dialogs and the bars'
   * selection follow their items; what the save changed or removed is let go.
   */
  function swapOutside(base: Project | null, remote: Project, next: Project) {
    drag.foreign("outside save");
    const before = project;
    if (!before || !base) {
      project = next;
      return;
    }
    const wall = selection ? jobEdits.hold("parts", selection.part, true) : null;
    const dormer = dormerSel !== null ? jobEdits.hold("dormers", dormerSel, true) : null;
    const opening = openSel ? jobEdits.hold(SEL_LIST[openSel.kind], openSel.index, true) : null;
    const manual = manualSel ? jobEdits.hold(manualSel.kind === "line" ? "manual_lines" : "manual_faces", manualSel.index, true) : null;
    project = next;
    jobEdits.outside(base, remote, plain(before), next);
    if (wall && selection) selection = wall.gone ? null : keptSelection(next, { part: wall.index, edge: selection.edge });
    if (dormer) dormerSel = dormer.gone ? null : dormer.index;
    if (opening && openSel) openSel = opening.gone ? null : { kind: openSel.kind, index: opening.index };
    if (manual && manualSel) manualSel = manual.gone ? null : { kind: manualSel.kind, index: manual.index };
  }

  /** AI changes since the user's last own action or accept, newest first. */
  // The steps the core says wait (`review.steps`): an AI step the user
  // reverted by itself is none, the others still are (UX-1501). An older
  // bridge without the list: every AI entry since the oldest one.
  const pendingList = $derived(
    review ? activity.filter((a) => isAi(a.actor) && (review!.steps ? review!.steps.includes(a.ts_ms) : a.ts_ms >= review!.since_ts_ms)) : [],
  );
  const pending = $derived(pendingList.length ? pendingList[0] : null);
  /** The steps Revert all takes out: the AI's creation of the job stays (HUNT-1601, ERR-1705). */
  const revertSteps = $derived(pendingList.filter((a) => !a.created).length);
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
  // Input the calculation left out (a building with a wall shorter than
  // 1 cm, a dormer with a bad value): the figures miss it, so no export or
  // offer from them either (review UX-002).
  const blockedReason = $derived(
    !result || roofError ? t("tool.exportBlocked") : result.problems?.length ? t("tool.exportBlockedProblems") : null,
  );
  $effect(() => {
    if (blockedReason) exporting = false;
  });
  // The banner's change line and the plan's marks: the approved state
  // against the job now (UX-2001); each step's own change is in the AI tab.
  const pendingChanges = $derived(bannerChanges(review, pendingList));
  const highlight = $derived.by(() => {
    if (!project) return new Set<string>();
    if (focusChange) return changedEdges([focusChange], project);
    return changedEdges(pendingChanges, project);
  });
  const highlightItems = $derived(changedItems(focusChange ? [focusChange] : pendingChanges));
  /** What the AI changed on each highlighted wall, written beside it in the plan (UX-017). */
  const reviewNotes = $derived(review ? wallNotes(focusChange ? [focusChange] : pendingChanges) : new Map<string, string>());
  /**
   * The roof of the version before the pending AI changes, from the core, so
   * a pitch, eave or gable change shows the previous roof lines dashed
   * (UX-017). Fetched once per review state; without it the plan shows only
   * the outlines, as before.
   */
  let beforeResult = $state<CalcResult | null>(null);
  const beforeKey = $derived(reviewBeforeKey(review));
  $effect(() => {
    const key = beforeKey;
    beforeResult = null;
    const before = untrack(() => review?.before);
    if (!key || !before) return;
    let live = true;
    api
      .calc($state.snapshot(before) as Project)
      .then((r) => {
        if (live) beforeResult = r;
      })
      // A previous version the core cannot calculate has no lines to show.
      .catch(() => {});
    return () => {
      live = false;
    };
  });
  const ghost = $derived(
    project && review && showBefore
      ? ghostOf(focusChange ? [focusChange] : pendingChanges, review.before, project, result, beforeResult)
      : null,
  );
  const wallCount = $derived(project?.parts.reduce((n, p) => n + p.edges.length, 0) ?? 0);

  /**
   * A recalculation is on its way and the figures shown are of the roof
   * before the last edit: Results, the status bar and the views say so
   * (ERR-204). Set only after a moment, so a quick calculation does not flicker.
   */
  let calcPending = $state(false);
  let pendingTimer: ReturnType<typeof setTimeout> | undefined;
  /** How long a calculation runs before it is marked as pending, ms [no source: editor choice]. */
  const PENDING_MS = 250;

  /**
   * The window, chimney or roof opening just placed, and the calculation
   * before it: the first result after it says at once when the core finds
   * it on no roof face (audit 2026-11-23 UX-1401; kept, as MCP keeps it).
   */
  type Placed = { kind: "window" | "chimney" | "opening"; index: number };
  /**
   * Every placement not checked yet: several made before one calculation
   * answers are each checked against the first result after them, and a
   * warning toast goes when a later result no longer has its warning (the
   * object was moved or Undo took it away; audit 2026-11-23 UI-1502).
   */
  let placed: (Placed & { after: number })[] = [];
  let offToasts: (Placed & { toast: number })[] = [];
  function placedOn(kind: "window" | "chimney" | "opening", index: number) {
    placed.push({ kind, index, after: calcSeq });
  }
  /** The placement toasts after the current result `r`. */
  function checkPlaced(r: CalcResult | null, seq: number) {
    offToasts = offToasts.filter((o) => {
      // Closed already (by its timer or the user): nothing to take back.
      if (!toasts.some((x) => x.id === o.toast)) return false;
      if (offRoofWarning(r, o.kind, o.index)) return true;
      dismiss(o.toast);
      return false;
    });
    const due = placed.filter((p) => seq > p.after);
    placed = placed.filter((p) => seq <= p.after);
    for (const p of due) {
      const off = offRoofWarning(r, p.kind, p.index);
      if (!off || offToasts.some((o) => o.kind === p.kind && o.index === p.index)) continue;
      const toast = notify({ kind: "warning", code: off.kind, title: t("res.warn.title"), message: warningText(off) });
      ownToasts.add(toast);
      offToasts.push({ kind: p.kind, index: p.index, toast });
    }
  }

  async function recalc() {
    if (!project) return;
    const seq = ++calcSeq;
    clearTimeout(pendingTimer);
    pendingTimer = setTimeout(() => seq === calcSeq && (calcPending = true), PENDING_MS);
    try {
      const r = await api.calc($state.snapshot(project) as Project);
      if (seq === calcSeq) (result = r), (calcFailure = null);
      if (seq === calcSeq) checkPlaced(r, seq);
    } catch (e) {
      if (seq === calcSeq) (result = null), (calcFailure = describeError(e, "calc"));
      notifyError(e, "calc");
    } finally {
      if (seq === calcSeq) {
        clearTimeout(pendingTimer);
        calcPending = false;
      }
    }
  }

  /**
   * The engine's recalculation the job's stored result is, as the core
   * says (`Store::recalculation`): the one fact the result card's
   * "recalculated" line and the history's "put the offer's figures back"
   * come from (audit 2026-11-22 ERR-802, UI-802, ARCH-802). `null` for any
   * other stored result.
   */
  let recalculation = $state<Recalculation | null>(null);
  /** `taken_ms` of the stored result on disk that `recalculation` was asked for (`undefined`: not asked yet). */
  let recalcFor: number | null | undefined;

  /**
   * Reads the history, the review and the recalculation. `hold`: asked once
   * they are read; when it says so nothing is shown and the answer is
   * `false` (a reload that a drag overtook: the AI's review banner must not
   * appear in the middle of the drag, jobedits.ts rule 7).
   */
  /** The last read of the history went through: `activity` is the log as it is (for `seenTs`, design B). */
  let activityOk = false;
  /** The last read of the review status succeeded (a failed one keeps the banner as it was, ERR-1903). */
  let reviewRead = true;
  /**
   * The reads of history and review can overtake each other (a reload, an
   * Accept, a review action): only the newest one's answer is shown, so a
   * slow read of an older version never brings back a banner Accept just
   * closed (audit 2026-11-24 ERR-1905). An action that changes the review
   * `bump()`s it before and after its call.
   */
  const activityReads = sequence();
  /** The newest read under way, which an obsolete read waits for, so its caller goes on with the newest state. */
  let newestActivity: { k: number; run: Promise<boolean> } | null = null;
  /** The one retry after a failed read (ERR-1903). */
  let activityRetry: ReturnType<typeof setTimeout> | undefined;
  /** The open's `reviewLater`, when its review read failed: run once a later read gets the review (OPENFAIL). */
  let reviewLaterWaiting: (() => void) | null = null;
  /** The notice that the review could not be read was shown, since the last read that got it. */
  let reviewUnreadSaid = false;
  /**
   * `reviewLater`: the job opens without waiting for the review status (two
   * roof calculations, up to the work bound on a heavy job, HUNT-1903): the
   * banner comes when it arrives, and `reviewLater` runs then.
   */
  type ActivityOptions = { reviewLater?: () => void; retry?: boolean };
  /** How long after a failed read of history or review it is tried once more, ms. */
  const ACTIVITY_RETRY_MS = 1500;
  function refreshActivity(hold?: () => boolean, opts: ActivityOptions = {}): Promise<boolean> {
    const k = activityReads.next();
    const run = readActivity(k, hold, opts);
    newestActivity = { k, run };
    return run;
  }
  async function readActivity(k: number, hold: (() => boolean) | undefined, opts: ActivityOptions): Promise<boolean> {
    clearTimeout(activityRetry);
    let reviewFailed = false;
    let activityFailed = false;
    const reviewAnswer = api.reviewStatus(id).catch(() => ((reviewFailed = true), null));
    const [a, rec] = await Promise.all([
      api.projectActivity(id).catch(() => ((activityFailed = true), [] as ActivityItem[])),
      api.resultRecalculation(id).then(
        (r) => ({ r }),
        () => null,
      ),
    ]);
    const r = opts.reviewLater ? null : await reviewAnswer;
    if (!activityReads.current(k)) {
      // A newer read was started (or an action changed the review): its
      // answer is the one shown; the caller goes on once it is in.
      const newer = newestActivity && newestActivity.k !== k ? newestActivity.run : null;
      if (newer) await newer.catch(() => {});
      return !hold?.();
    }
    if (hold?.()) return false;
    // A failed read never replaces what is known: the banner, Accept and
    // Revert stay, only marked unchecked, and the read is tried once more
    // (ERR-1903).
    if (activityFailed) activityOk = false;
    else {
      activity = a;
      activityOk = true;
    }
    if (rec) recalculation = rec.r;
    const retry = () => {
      if (opts.retry || destroyed) return;
      clearTimeout(activityRetry);
      activityRetry = setTimeout(() => {
        if (!destroyed) void refreshActivity(undefined, { retry: true });
      }, ACTIVITY_RETRY_MS);
    };
    if (opts.reviewLater) {
      const later = opts.reviewLater;
      void reviewAnswer.then((late) => {
        if (destroyed || !activityReads.current(k)) return;
        if (reviewFailed) {
          reviewRead = false;
          // The banner and the dock's switch to the AI tab come with the
          // retry's review (audit 2026-11-24 OPENFAIL).
          reviewLaterWaiting = later;
          retry();
          return;
        }
        reviewRead = true;
        reviewUnreadSaid = false;
        review = late;
        later();
      });
    } else if (reviewFailed) {
      reviewRead = false;
      // The retry failed too: whether AI changes wait is not known, so say
      // so once instead of showing no banner as if none waited (OPENFAIL).
      if (opts.retry && !reviewUnreadSaid && !destroyed) {
        reviewUnreadSaid = true;
        notify({ kind: "warning", title: t("review.unread") });
      }
    } else {
      reviewRead = true;
      reviewUnreadSaid = false;
      review = r;
      const later = reviewLaterWaiting;
      reviewLaterWaiting = null;
      later?.();
    }
    if (activityFailed || (!opts.reviewLater && reviewFailed) || !rec) retry();
    return true;
  }
  /** The newest entry of the history just read, when the read went through. */
  const newestRead = (): number | null => (activityOk ? activity.reduce((m, a) => Math.max(m, a.ts_ms), 0) : null);

  /**
   * After a save: the editor's own saves write no change-log entry, so a
   * stored result that changed on disk (Store result, an Undo or Redo
   * across one) is asked about again. The card then never credits the
   * user's own result to the recalculation, and an Undo that brings the
   * engine's result back says so (ERR-802, UI-802).
   */
  async function syncRecalculation(taken: number | null) {
    if (taken === recalcFor) return;
    recalcFor = taken;
    const rec = await api.resultRecalculation(id).catch(() => null);
    if (!destroyed && recalcFor === taken) recalculation = rec;
  }

  /** Only the AI's creation of the job waits: the banner offers Delete job instead of Revert (HUNT-1601). */
  const onlyCreation = $derived(pendingList.length > 0 && pendingList.every((a) => a.created));
  /** The job was deleted from the banner: nothing of it is saved or read again (HUNT-1601). */
  let deleted = false;

  /**
   * The banner's Delete job, when only the AI's creation of the job waits
   * for review (owner decision 3 of round 17): the job goes to the trash
   * after the same question the library asks, and the editor is left. The
   * library's trash brings it back for 30 days.
   */
  async function deleteJobNow() {
    const name = project?.name.trim() || id;
    if (!(await asking(async () => await confirm(t("home.confirmDelete", { name }))))) return;
    if (destroyed) return;
    // An AI step that came in while the question was open is not deleted
    // with the job unseen: read the review again, and when an AI step
    // besides the creation waits by now, delete nothing and say so
    // (UI-1702, design E).
    await refreshActivity();
    if (destroyed) return;
    // A failed read is "could not check", not "nothing waits": nothing is
    // deleted (audit 2026-11-24 errors To-check 2).
    const unchecked = !activityOk || !reviewRead;
    if (unchecked || pendingList.some((a) => !a.created)) {
      info(t(unchecked ? "review.deleteUnchecked" : "review.deleteChanged"));
      // The pressed Delete job may be gone (the new step brings Revert):
      // the focus goes to the banner, not to the page (UI-1804).
      await tick();
      if (destroyed) return;
      if (bannerEl?.isConnected) bannerEl.focus();
      else document.getElementById(dockIds.tab(dock))?.focus();
      return;
    }
    deleted = true;
    clearTimeout(saveTimer);
    try {
      await saveQueue;
      const gone = await api.deleteProject(id);
      // Gone on purpose: leaving asks about no unsaved edit.
      saveState = "saved";
      // The library says so, with its Restore at once (UX-1703).
      handOverDeleted(gone, name);
      onback();
      // The focus goes to the library's list (or its search), not to the page (UI-1702).
      focusLibrary();
    } catch (e) {
      deleted = false;
      notifyError(e, "delete", name);
      // A reload an outside save asked for while the delete was out was
      // dropped (`load` returns while `deleted`): read the job now (HUNT-1706).
      if (!destroyed) load(false);
    }
  }

  /** After the editor is left by Delete job: the library's focused row, else its search field. */
  function focusLibrary(tries = 10) {
    requestAnimationFrame(() => {
      const el = document.querySelector<HTMLElement>("tbody tr[tabindex='0'], .search input");
      if (el) el.focus();
      else if (tries > 0) focusLibrary(tries - 1);
    });
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
    saveTimer = setTimeout(autosave, 500);
  }

  /**
   * The timer's save: not while a drag runs, which would write the half
   * dragged job (without the roof follow of the drop) for the AI and the
   * change detection to read; the drop saves anyway (TEST-505).
   */
  function autosave() {
    if (inputClosed()) saveTimer = setTimeout(autosave, 500);
    else void save();
  }

  /** Writes the job; resolves to whether the edits are on disk (errors are shown, never thrown). */
  function save(): Promise<boolean> {
    if (!project || deleted) return Promise.resolve(false);
    clearTimeout(saveTimer);
    saveState = "saving";
    inFlight++;
    saveGen++;
    const run = saveQueue.then(saveNow).finally(() => {
      inFlight--;
      if (!inFlight && reloadWanted && !drag.active) {
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
      let repaired = false;
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
            // Not into a drag: it is merged once the drag is over.
            await drag.settled();
            if (!project) return false;
            const base = JSON.parse(lastSaved) as Project;
            swapOutside(base, r.current, merge3(base, $state.snapshot(project) as Project, r.current));
            lastSaved = JSON.stringify(r.current);
            // The outside save's entries are in the job now (TEST-1905).
            mergedOutside = true;
            closeStaleDialogs();
            pushHistory();
            recalc();
            refreshActivity();
            continue;
          }
          repaired = !!r.repaired;
        } else {
          await api.saveProject(id, written);
        }
        // The store writes its own canonical form (no nulls, empty lists or
        // defaults); remember that, or the file-change event would come back
        // as an external edit and break undo. Not read back from the file: an
        // AI edit saved right after ours would then pass for our own.
        lastSaved = JSON.stringify(await api.normalizeProject(written));
        failedSave = null;
        // An edit made while this save was running is still unsaved.
        saveState = edits === seq ? "saved" : "dirty";
        // The other program's file the store could not read is gone now.
        if (repaired || outsideUnreadable) outsideOver(repaired);
        if (outsideNewer) newerOver();
        void syncRecalculation((written as Project).result_snapshot?.taken_ms ?? null);
        return true;
      }
    } catch (e) {
      // Keep the edits in memory; the status bar offers to try again.
      saveState = "failed";
      // Named: the notice may outlive the editor (UI-1503).
      failedSave = { code: toAppError(e).code, toast: notifyError(e, "save", project?.name.trim() || id) };
      return false;
    }
  }

  /** Steps ever added to the history: tells whether a drop became one (ERR-701). */
  let steps = 0;
  function pushHistory() {
    const s = snapshot();
    if (history[at] === s) return false;
    steps++;
    history = history.slice(0, at + 1);
    undoVia.length = at + 1;
    redoVia.length = at + 1;
    redoVia[at] = undefined;
    history.push(s);
    undoVia.push(undefined);
    redoVia.push(undefined);
    at = history.length - 1;
    historyLen = history.length;
    return true;
  }

  /** Records the current state as an undo step, recalculates and saves. */
  function commit() {
    // Not while a drag is open, unless it is the drag's own (jobedits.ts rule 7).
    drag.foreign("commit");
    // Not while Revert or the put-back is out: its answer replaces the job,
    // so the edit would be lost behind "Saved". The caller changed `project`
    // already: it goes back to the newest step, and the user is told
    // (backstop of round 12 fix design 1; the gates before it leave nothing).
    const putBack = () => {
      if (history[at]) project = JSON.parse(history[at]);
    };
    if (dropForeign(reviewing, "commit", putBack, refusedDuringAction)) return;
    const prev = history[at];
    if (!pushHistory()) return;
    // Edits under way follow their items past a delete or a copy (jobedits.ts).
    if (project && prev) jobEdits.own(() => JSON.parse(prev) as Project, project);
    recalc();
    scheduleSave();
  }

  /** Goes to undo step `i`; false when it was refused (a core action is out). */
  function restore(i: number): boolean {
    drag.foreign("undo step");
    if (dropForeign(reviewing, "undo step", () => {}, refusedDuringAction)) return false;
    const prev = history[at];
    at = i;
    project = JSON.parse(history[i]);
    if (project && prev) jobEdits.own(() => JSON.parse(prev) as Project, project);
    // The remembered pitches stay: a redo brings the gable back, and Eave
    // then gives the wall its own pitch again (UX-202).
    // The wall stays in the bar when the step kept it (UX-005).
    selection = keptSelection(project, selection);
    recalc();
    scheduleSave();
    return true;
  }

  /**
   * The step that closed a drawn outline, with the corners drawn: undoing
   * that step opens the draw bar again with them, so one wrong wall is
   * fixed without drawing the whole building again (UX-004).
   */
  let finished: { step: string; corners: Point2[] } | null = null;
  /** The corners the last undo reopened, to close the draw bar again on redo. */
  let reopened: string | null = null;

  /**
   * Undo and Redo take back the user's newest step: an edit still with the
   * core (a value typed just before) is that step, so they wait for it
   * first; else the press undid the step before it, the typed value landed
   * after it and Redo was lost (UX-501). Leaving waits for them too.
   */
  function afterEdits(step: () => void) {
    // A drag still held is the only writer of the job: the press is refused.
    if (dragState === "armed" || dragState === "moving") return;
    if (!drag.active && !jobEdits.size) return step();
    // A drop still with the core is the newest step too: one Undo right
    // after a building's drop undoes exactly that drop (ERR-603). A drop
    // the core refuses adds no step: the press is then dropped, else it
    // undid the step before the drag (ERR-701).
    const dropping = drag.active;
    const before = steps;
    void pendingEdits.track(
      calm().then(() => {
        if (dropping && steps === before) return;
        step();
      }),
    );
  }
  /** Resolves once no drag is open and no edit is with the core. */
  async function calm() {
    while (drag.active || jobEdits.size) {
      await drag.settled();
      await jobEdits.idle();
    }
  }
  const undo = () => afterEdits(undoNow);
  const redo = () => afterEdits(redoNow);
  /** Runs the next Undo / Redo press queued while a core action was out (UI-1704). */
  function runQueued() {
    const next = coreQueue.shift();
    if (next) (next === "undo" ? undo : redo)();
  }

  function undoNow() {
    // A step through the core is out: this press runs after its answer (UI-1704).
    if (coreStep) {
      coreQueue.push("undo");
      return;
    }
    if (at <= 0) {
      coreQueue = [];
      return;
    }
    const via = undoVia[at];
    if (via) {
      void stepThroughCore(at - 1, via);
      return;
    }
    const reopen = !drawing && finished && history[at] === finished.step ? finished.corners : null;
    const ok = restore(at - 1);
    if (reopen) {
      selection = null;
      placing = false;
      drawing = reopen.map((p) => ({ ...p }));
      reopened = JSON.stringify(reopen);
    }
    if (ok) runQueued();
    else coreQueue = [];
  }

  function redoNow() {
    if (coreStep) {
      coreQueue.push("redo");
      return;
    }
    if (at >= history.length - 1) {
      coreQueue = [];
      return;
    }
    const via = redoVia[at];
    if (via !== undefined) {
      void stepThroughCore(at + 1, { take: via });
      return;
    }
    // Redo of the closed outline: the building comes back, the draw bar goes.
    if (drawing && reopened === JSON.stringify($state.snapshot(drawing))) drawing = null;
    reopened = null;
    if (restore(at + 1)) runQueued();
    else coreQueue = [];
  }

  /**
   * Undo or Redo across a step the core made (`undoVia`, `redoVia`): a
   * review transaction like Revert, whose answer becomes step `to`; the
   * revert it logged is what the opposite press takes back. Each press
   * names the newest entry the editor has applied (`seenTs`): when the log
   * has a newer one (an AI saved during the press) the core refuses with
   * `revert_stale`, nothing is undone, the notice says so under the
   * press's own title and the job is read again, so the AI's save arrives
   * as a step of its own (audit 2026-11-24 ERR-1704, ARCH-1703, design B).
   * `revert_conflict` or `busy`: the notice says so and the step stays.
   */
  async function stepThroughCore(to: number, via: CoreUndo) {
    const from = at;
    let entry: number | null = null;
    let repaired = false;
    let failed: unknown = undefined;
    let done = false;
    const what = to < from ? "undo" : "redo";
    // Another press (Revert, the put-back) is out: refused with a word, as
    // a local step is then (UI-1704).
    if (reviewing.active || reviewPress) {
      refusedDuringAction();
      return;
    }
    coreStep = true;
    try {
      await reviewTransaction(
        what,
        async () => {
          const seen = seenTs ?? 0;
          try {
            const r = "take" in via ? await api.takeBackRevert(id, via.take, seen) : await api.revertAll(id, via.from, seen);
            entry = r.entry_ts_ms ?? null;
            answerEntry = entry;
            repaired = !!r.repaired;
            return r.project;
          } catch (e) {
            failed = e;
            throw e;
          }
        },
        () => {
          focusChange = null;
          if (repaired || outsideUnreadable) outsideOver(repaired);
        },
        () => {
          done = true;
          // The transaction held the editor: nothing moved the history meanwhile.
          if (at !== from || !history[to]) {
            pushHistory();
            return;
          }
          history[to] = snapshot();
          at = to;
          selection = project ? keptSelection(project, selection) : null;
          if (to < from) {
            redoVia[to] = entry ?? undefined;
          } else {
            undoVia[to] = entry === null ? undefined : { take: entry };
          }
        },
      );
    } finally {
      coreStep = false;
    }
    if (destroyed) return;
    if (!done) {
      // Refused, failed or not run: the presses made meanwhile are dropped
      // with it (its notice is the one word on them).
      coreQueue = [];
      // The log has an entry the editor has not shown: read it, so the
      // AI's save comes in as a step of its own and the next press is right.
      if (failed !== undefined && toAppError(failed).code === "revert_stale") load(false);
      return;
    }
    runQueued();
  }
  /** A step through the core is out (`stepThroughCore`), and the Undo / Redo presses made meanwhile, in order (UI-1704). */
  let coreStep = false;
  let coreQueue: ("undo" | "redo")[] = [];

  /**
   * The reload an outside save asked for during a drag, run by the drag
   * session when the drag is over (DragSession.defer, TEST-702): at once,
   * or after the drop's own save.
   */
  function reloadAfterDrag() {
    if (inFlight) reloadWanted = true;
    else load(false);
  }

  /**
   * One load at a time: a file event while one runs reads once more after
   * it, so an older read never lands after a newer one and passes for an
   * outside save (audit 2026-11-24 ERR-1906), and file events do not pile
   * up calculating reads (HUNT-1903). As `reloadWanted` does for saves.
   */
  let loading: Promise<void> | null = null;
  let loadAgain = false;
  /** The newest entry a reload of the unchanged job did not pass `seenTs` over (TEST-1905). */
  let heldFor: number | null = null;
  /** A save merged an outside save in: its entries are in the job the editor has (TEST-1905). */
  let mergedOutside = false;
  function load(initial: boolean): Promise<void> {
    if (loading) {
      if (!initial) loadAgain = true;
      return loading;
    }
    const run = loadOnce(initial).finally(() => {
      loading = null;
      if (loadAgain && !destroyed) {
        loadAgain = false;
        void load(false);
      }
    });
    loading = run;
    return run;
  }

  async function loadOnce(initial: boolean) {
    if (deleted) return;
    if (!initial && drag.active) {
      // An outside save of this job during a drag is read once the drag is
      // over (jobedits.ts rule 7); its review banner comes with it.
      drag.defer("reload", reloadAfterDrag);
      return;
    }
    if (!initial && reviewing.active) {
      // The same during a review action: its answer is the job (design A1).
      reviewing.defer("reload", reloadAfterDrag);
      return;
    }
    const gen = saveGen;
    // The recalculation this open made, until the notice is shown: an editor
    // left before that keeps it for the next editor of the job (ERR-902).
    let recalculated: RefreshedEntry | null = null;
    try {
      // A result an older engine stored follows the corrected calculation
      // before the job opens (owner decision 2026-11-21); the editor says
      // so (audit UX-702c). A failed check opens the job as it is.
      const refreshing = initial
        ? api.refreshStoredResult(id).catch((e) => {
            console.warn("[tectora] refresh_stored_result:", e);
            return null;
          })
        : null;
      // What reading the job leaves out of it (a stored result with an
      // impossible time, HUNT-802), asked as the job opens and told below,
      // in this editor only (UX-901).
      const warned = initial ? api.loadWarnings(id).catch(() => [] as AppError[]) : null;
      recalculated = await refreshing;
      // The user left this job while it was read: nothing of it is shown in
      // the editor of the next one (audit 2026-11-22 HUNT-801).
      if (destroyed) return;
      const p = await api.loadProject(id);
      if (destroyed) return;
      // Readable again: nothing of the other program's file is replaced.
      if (!initial && outsideUnreadable) outsideOver(false);
      if (!initial && outsideNewer) newerOver();
      const json = JSON.stringify(p);
      // A drag that began while this was read: read again after it. The
      // job opens without waiting for the review's figures (HUNT-1903):
      // the banner comes when they are in.
      const hold = () => destroyed || (!initial && drag.active);
      const opened = initial
        ? refreshActivity(hold, {
            reviewLater: () => {
              if (pending && !typingInDock()) dock = "ai";
              void migrateLocalReview();
            },
          })
        : refreshActivity(hold);
      if (!(await opened)) {
        if (!destroyed) drag.defer("reload", reloadAfterDrag);
        return;
      }
      if (destroyed) return;
      recalcFor = p.result_snapshot?.taken_ms ?? null;
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
      if (!initial && json === lastSaved) {
        // The job the editor has: every entry read is in it (design B),
        // except an AI entry logged before its file was written (the store
        // logs first): the job read is still the one before it. Not passed
        // over once, so the AI's save that follows comes in as a step the
        // core undoes (audit 2026-11-24 TEST-1905). A save merged with an
        // outside one has those entries in it already.
        const newest = newestRead();
        const unseenAi = newest !== null && activity.some((a) => a.ts_ms > (seenTs ?? 0) && isAi(a.actor) && !a.undone && !a.created);
        if (unseenAi && !mergedOutside && heldFor !== newest) heldFor = newest;
        else {
          heldFor = null;
          mergedOutside = false;
          seenTs = newest ?? seenTs;
        }
        retryFailedSave();
        return;
      }
      if (!initial && drag.active) {
        // An outside save of this job during a drag waits for its end: the
        // drag is the only writer of the job meanwhile (jobedits.ts rule 7).
        drag.defer("reload", reloadAfterDrag);
        return;
      }
      if (!initial && reviewing.active) {
        reviewing.defer("reload", reloadAfterDrag);
        return;
      }
      // Indices in an open menu refer to the old project.
      menu = null;
      // An external edit while local edits are not saved yet (waiting for
      // the timer, or a save that failed): keep both (the external one wins
      // where both changed the same value). The state stays "dirty" or
      // "failed", so the merged edits are still to be saved (ERR-002).
      const merged = !initial && (saveState === "dirty" || saveState === "failed") && project && lastSaved;
      const next = merged ? merge3(JSON.parse(lastSaved), $state.snapshot(project) as Project, p) : p;
      // Edits under way, dialogs and the selection follow their items (jobedits.ts).
      if (initial) project = next;
      else swapOutside(lastSaved ? (JSON.parse(lastSaved) as Project) : null, p, next);
      lastSaved = json;
      // The AI's entries the editor had not applied before this read (UX-1601, ERR-1702).
      const seenBefore = seenTs;
      seenTs = newestRead() ?? seenTs;
      heldFor = null;
      mergedOutside = false;
      pruneEavePitches(project!.parts);
      // A dialog open on a part the outside save removed or changed must not
      // act on whatever part now has its index (ERRU-010).
      if (!initial) closeStaleDialogs();
      if (initial) {
        history = [snapshot()];
        at = 0;
        historyLen = 1;
        // Only a job with nothing to calculate starts in the drawing tool: one of
        // hand-drawn faces only is a roof to look at (UX-301).
        if (!hasBuilding(p)) drawing = [];
        if (pending) dock = "ai";
        // Or the notice an editor of this job left before it could show it,
        // while the stored result still is that recalculation (ERR-902).
        const missed = takeMissedNotice(id);
        const tell = recalculated ?? (missed && recalculation?.entry_ts_ms === missed.entry_ts_ms ? missed : null);
        recalculated = null;
        if (tell) noticeRecalculated(tell);
        // Said once, as a warning naming the job (UX-901).
        const name = project?.name.trim();
        void warned?.then((ws) => {
          if (!destroyed) ws.forEach((w) => ownToasts.add(notifyWarning(w, name ? t("snap.titleOf", { name }) : t("snap.title"))));
        });
      } else {
        // External change (AI agent, CLI): keep it undoable and show it.
        // Merged local edits are still to be saved (the timer is running).
        // Undo of an AI's save is the banner's Revert from its oldest new
        // entry, logged, never an unlogged save of the job before it
        // (UX-1601, design A3).
        const fresh = seenBefore !== null && activityOk ? activity.filter((a) => a.ts_ms > seenBefore && isAi(a.actor) && !a.undone && !a.created) : [];
        if (pushHistory() && fresh.length) undoVia[at] = { from: fresh[fresh.length - 1].ts_ms };
        // Not while the user types in the dock: switching the tab would
        // remove the field and the typed text with it (UI-002).
        if (pending && !typingInDock()) {
          dock = "ai";
          // A drawer over the banner (narrow window): closed, so the
          // banner's Accept and Revert show; the AI tab is the one it
          // opens with (UI-1805).
          if (dockOpen && narrowDock()) {
            const inDock = !!document.activeElement?.closest("aside");
            dockOpen = false;
            if (inDock) void tick().then(() => bannerEl?.isConnected && bannerEl.focus());
          }
        }
        retryFailedSave();
      }
      recalc();
    } catch (e) {
      if (destroyed) return;
      if (initial || !project) {
        loadError = describeError(e, "load");
        loadErrorCode = toAppError(e).code;
      } else if (toAppError(e).code === "not_found") {
        // Deleted outside the app while open: keep the edits, offer to save.
        saveState = "failed";
        notifyError(e, "load");
      } else if (["unreadable", "too_many", "newer_format"].includes(toAppError(e).code)) {
        // A save started while this was read: it may have replaced the file
        // already (and said so, ERR-1502); read again after it instead of
        // warning about a save that is done.
        if (inFlight) {
          reloadWanted = true;
          return;
        }
        if (gen !== saveGen) {
          load(false);
          return;
        }
        // Another program wrote a file Tectora cannot read: not the open-time
        // advice (remove the data, restore a backup), but what the next save
        // does (audit 2026-11-23 ERR-1401; the repair is ARCH-1302).
        // Its own title (the job stays open, UX-1502), and it stays until
        // the save or revert that replaces the file, a good read or the
        // editor's end (UX-1504).
        const d = describeError(e, "load");
        const name = project.name.trim() || id;
        if (d.code === "newer_format") {
          // A newer Tectora's file is never written over: not the "will be
          // replaced" text, and not "could not open" for a job that is open
          // (UI-1603, HUNT-1604).
          if (outsideUnreadable) outsideOver(false);
          if (outsideNewer) return;
          outsideNewer = true;
          outsideNewerNotice = notify({ kind: "warning", code: d.code, title: t("err.ofJob", { name, title: t("editor.outsideTitle") }), message: t("editor.outsideNewer"), detail: d.detail, sticky: true });
          ownToasts.add(outsideNewerNotice);
          return;
        }
        if (outsideNewer) newerOver();
        outsideUnreadable = true;
        outsideNotice = notify({ kind: "warning", code: d.code, title: t("err.ofJob", { name, title: t("editor.outsideTitle") }), message: t("editor.outsideUnreadable"), detail: d.detail, sticky: true });
        ownToasts.add(outsideNotice);
      } else {
        notifyError(e, "load", project.name.trim() || id);
      }
    } finally {
      // Not shown here: the user left the job while it opened.
      if (recalculated) keepMissedNotice(id, recalculated);
    }
  }

  /** The history entry the AI tab opens (the recalculation notice's button). */
  let shownEntry = $state<number | null>(null);

  /**
   * The notices of this job the editor showed (the recalculation, load
   * warnings): they name the job and close with the editor, so they never
   * stand in the next job's editor (audit 2026-11-23 UI-1002).
   */
  const ownToasts = new Set<number>();

  /**
   * Notices that quote the job's stored result (the Job tab's "kept its
   * earlier result"), with the stored result they quote: once it changes
   * (the put-back, Undo, an AI save) they close, so no toast stands with
   * old figures next to the card (UI-1202). The next comparison shows a
   * fresh one if the figures still differ.
   */
  const resultToasts = new Map<number, string>();
  const storedKey = () => JSON.stringify(project?.result_snapshot ?? null);
  function resultToast(toast: number) {
    resultToasts.set(toast, storedKey());
    // Closes with the editor as every notice of the job: its button stores
    // into this job, never into the next one opened (HUNT-1302).
    ownToasts.add(toast);
  }
  $effect(() => {
    const now = storedKey();
    untrack(() => {
      for (const [toast, quoted] of resultToasts) {
        if (quoted === now) continue;
        dismiss(toast);
        resultToasts.delete(toast);
      }
    });
  });

  /** One neutral notice that opening the job recalculated its stored result, with the way to the old figures. */
  function noticeRecalculated(entry: RefreshedEntry) {
    const name = project?.name.trim();
    ownToasts.add(notify({
      kind: "info",
      title: name ? t("snap.titleOf", { name }) : t("snap.title"),
      message: refreshedNotice(entry.summary),
      action: {
        label: t("snap.refreshedShow"),
        run: () => {
          // Not after the job was left: the button would switch the panel of an editor that is gone (HUNT-801).
          if (destroyed) return;
          // The panel is opened too: in a narrow window it is closed until asked for (UI-801).
          dock = "ai";
          dockOpen = true;
          shownEntry = entry.entry_ts_ms;
        },
      },
    }));
  }

  /**
   * The stored result as the engine recalculated it (audit UX-702b): the
   * card then says so and keeps the offer's date. Only while the job's
   * stored result is that very result, by the core's word and by its
   * `taken_ms` here, so an edit not saved yet (Store result, Undo) shows
   * at once.
   */
  const recalculatedResult = $derived(recalculation && project?.result_snapshot?.taken_ms === recalculation.taken_ms ? recalculation : null);
  /** A received job whose sender's engine recalculated its stored result (UX-1003): told on the card, without the put-back. */
  const receivedResult = $derived(recalculatedResult ? null : receivedRecalculation(activity, project));

  let dockEl: HTMLElement | undefined = $state();
  /** A text field in the side panel has the focus. */
  function typingInDock(): boolean {
    const a = document.activeElement;
    return !!dockEl && !!a && dockEl.contains(a) && (a instanceof HTMLInputElement || a instanceof HTMLTextAreaElement || a instanceof HTMLSelectElement);
  }

  /** Follow mode must not take the user out of unfinished work (UX-001). */
  function busy(): boolean {
    if ((drawing?.length ?? 0) > 0 || placing !== false || manualDraft.length > 0) return true;
    // A drag from its press on, also a corner's (ERR-601).
    if (drag.active || reviewing.active || menu) return true;
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
    // Revert or the put-back is with the core: its answer (also `busy`) is
    // the user's to see before the job is left. Follow mode stays; the
    // user's own ways out wait for it, at most the core's lock wait
    // (ERR-1105, round 12 fix design 1).
    // The whole press counts, its prepare phase too: Back pressed then was
    // dropped without a word (ERR-1203, round 13 design A).
    if (reviewPress || reviewing.active) {
      if (mode === "quiet") return false;
      await reviewPress;
      await reviewing.settled();
    }
    // A footprint being drawn is not in the job yet: leaving drops it, so
    // ask as Escape does (UX-013). Follow mode (not the user's) never drops it.
    const pts = drawing?.length ?? 0;
    if (pts >= 2) {
      if (mode === "quiet") return false;
      // Tauri's dialog plugin makes confirm asynchronous: always await it.
      if (!(await confirm(t("draw.confirmCancel", { n: pts })))) return false;
    }
    // A drag under way ends where it is: its pause would hold every edit
    // below until a release that may never come (ERR-501).
    if (dragging()) cutDrag();
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

  /** The editor was left: answers that come later change nothing (HUNT-801). */
  let destroyed = false;
  onDestroy(() => {
    destroyed = true;
    clearTimeout(activityRetry);
    activityReads.bump();
    for (const toast of ownToasts) dismiss(toast);
    unlisten?.();
    cutDrag();
    releaseLeaveGuard(leaveGuard);
    if (!deleted && (saveState === "dirty" || saveState === "failed")) save();
  });

  /** Accept or Revert is running: a second press (double-click) must not run it twice (UI-002). */
  let reviewBusy = $state(false);
  const reviewAction = exclusive((run: () => Promise<void>) => run(), (b) => (reviewBusy = b));
  const accept = () => reviewAction(acceptNow);
  let bannerEl = $state<HTMLElement>();
  /**
   * The banner's Revert and Accept, pressed from the keyboard, keep the
   * focus: on the button while it waits (a `busy` answer keeps the banner),
   * else on the banner, or the dock's open tab once the banner is gone
   * (UI-1103).
   */
  const pressBanner = (run: () => void | Promise<void>) =>
    pressKeepingFocus(run, () => (bannerEl?.isConnected ? bannerEl : document.getElementById(dockIds.tab(dock))));
  /** Reverts the pending AI changes from the banner, or the latest one from the AI tab. */
  const revert = (all = false, expectTs?: number) => reviewAction(() => revertNow(all, expectTs));
  /** Puts the offer's own figures back in place of the engine's recalculation logged at `entryTs` (the AI tab's button on that entry). */
  const restoreOffer = (entryTs: number) => reviewAction(() => restoreOfferNow(entryTs));

  /**
   * Takes in the job the core wrote for a Revert or a put-back (`p`): the
   * job is replaced by it as it is. Nothing local can be merged into it,
   * as the editor took no edit while the call was out (`reviewing`, design
   * A1; owner decision 2026-11-23, retracting the round-10 merge of
   * ERR-901, which lost or resurrected edits: HUNT-1001, UX-1001, UX-1002,
   * ERR-1004). Edits under way, dialogs and the selection follow their
   * items (`swapOutside`); `base` is the file as last read or written
   * before the call. `entry`: the change-log entry the call logged, now
   * the newest one the job has (`seenTs`, design B).
   */
  function applyCoreAnswer(base: string | null, p: Project, entry: number | null) {
    swapOutside(base ? (JSON.parse(base) as Project) : null, p, p);
    lastSaved = JSON.stringify(p);
    saveState = "saved";
    if (entry !== null) seenTs = entry;
  }
  /** The entry the running review call logged, set by its `call` (`applyCoreAnswer`). */
  let answerEntry: number | null = null;

  /**
   * How long (ms) a press of Revert or the put-back waits for the edits
   * made before it to be answered by the core and saved: the core's own
   * wait for the job's lock (store.rs `action_wait`). Past it the press
   * gives up with `busy` and changes nothing (ERR-1102).
   */
  const PREPARE_MS = 3000;
  /**
   * The budget of this press. Only the dev build (the E2E run) reads a
   * longer one a test set on `window.__e2e.prepareMs`, for the tests whose
   * steps inside the budget are wall-clock (TEST-1306); a release build
   * always waits PREPARE_MS.
   */
  const prepareMs = (): number => (import.meta.env.DEV && Number((globalThis as { __e2e?: { prepareMs?: unknown } }).__e2e?.prepareMs)) || PREPARE_MS;

  /**
   * Revert, Revert all and the put-back of the offer's figures: a short
   * transaction like a drag (dragsession.ts `CoreAction`), in two phases
   * (round 12 fix design 1).
   * 1. Prepare, input still open, at most PREPARE_MS: the value being
   *    typed at the press is stored, the edits still with the core are
   *    waited for (a status change, Store, a finished footprint:
   *    HUNT-1101) and the job saved, again until, checked at one moment,
   *    the editor is quiet (`quiet`: nothing with the core, the file has
   *    every edit, no text typed and not stored, no question, menu or
   *    dialog open; round 13 design A). Past the deadline the press says
   *    `busy_edits` under its own title and changes nothing; a save still
   *    under way lands as the user's own edit. A failed save stops it (it
   *    said why). Leaving waits for the whole press (`reviewPress`).
   * 2. In the same tick the call goes out: until the answer is in, keys,
   *    fields, bar buttons, plan presses, window keys and toast actions are
   *    refused (`anyWriterOpen`), the core's answers to earlier edits wait,
   *    and the status bar says so. The core waits for the job's lock a few
   *    seconds at most and answers `busy`, having changed nothing: the
   *    button stays and a note says why, naming the job (it may have been
   *    left meanwhile, ERR-1105).
   */
  /** The running press of Revert or the put-back, prepare phase and call; leaving waits for it (ERR-1203). */
  let reviewPress: Promise<void> | null = null;
  /** Questions (async confirm under Tauri) open now whose answer ends in an edit (HUNT-1204). */
  let questions = 0;
  /** Runs a confirm-then-edit as one: the prepare phase waits for its answer and its edit, no tick between (HUNT-1204). */
  async function asking<R>(run: () => Promise<R>): Promise<R> {
    questions++;
    try {
      return await run();
    } finally {
      questions--;
    }
  }
  /**
   * Nothing of the user's is under way: no edit with the core or unsaved, no
   * drag, no value typed and not stored, no question open, no menu, no
   * dialog (round 13 design A: what "ready" means for the claim).
   */
  const quiet = () =>
    pendingEdits.size === 0 && saveState === "saved" && !drag.active && !isTyping(document.activeElement) && questions === 0 && !menu && openModals() === 0;

  /** What a press is, for its notices: Revert of several steps has its own (plural) title (UI-1301). */

  function reviewTransaction(what: ReviewPress, call: () => Promise<Project>, after: (p: Project) => void | Promise<void>, place?: () => void): Promise<void> {
    if (reviewing.active || reviewPress) return Promise.resolve();
    const press: Promise<void> = prepareReview(what, call, after, place).finally(() => {
      if (reviewPress === press) reviewPress = null;
    });
    reviewPress = press;
    return press;
  }

  async function prepareReview(what: ReviewPress, call: () => Promise<Project>, after: (p: Project) => void | Promise<void>, place?: () => void) {
    const name = project?.name.trim() || id;
    const budget = prepareMs();
    const deadline = Date.now() + budget;
    const LATE = Symbol("late");
    const within = <T,>(p: Promise<T>): Promise<T | typeof LATE> =>
      Promise.race([p, new Promise<typeof LATE>((r) => setTimeout(() => r(LATE), Math.max(0, deadline - Date.now())))]);
    // The editor was left before the call could go out: say so, as a
    // failure after leaving is said (ERR-1203; ERR-1105).
    const left = () => notifyError({ code: "left_before_action", id, detail: "the job was closed before the call went out" }, what, name);
    // The field focused at the press is stored, once (UI-1201): a field
    // entered later is never blurred by the loop; while it holds typed
    // text the claim waits (`quiet`).
    commitFocused();
    await tick();
    for (;;) {
      if (Date.now() >= deadline) break;
      if ((await within(pendingEdits.settled())) === LATE) break;
      if ((await within(drag.settled())) === LATE) break;
      await tick();
      if (destroyed) {
        left();
        return;
      }
      if (!project) return;
      // Checked at one moment, and the call claimed in the same tick: no
      // answer of an earlier edit can land between the two.
      if (quiet()) return runReview(what, name, call, after, place);
      if (saveState !== "saved" && !pendingEdits.size && !drag.active) {
        const ok = await within(save());
        if (ok === LATE) break;
        // The save showed its error; nothing is reverted over edits not on disk.
        if (!ok) return;
      } else {
        // An edit is under way (typed, asked, with the core): let it finish first.
        await within(new Promise((r) => setTimeout(r, 20)));
      }
    }
    if (destroyed) {
      left();
      return;
    }
    // The edits before the press did not settle in time: typed, still with
    // the core, or their save waits for another writer's lock (UX-1201;
    // `busy` is the core's own lock answer). When they did and only a
    // question, a menu or a dialog held the claim, the notice says that
    // instead: the status bar says saved by then (UX-1301).
    const settled = pendingEdits.size === 0 && saveState === "saved" && !drag.active && !isTyping(document.activeElement);
    const held = questions > 0 || !!menu || openModals() > 0;
    if (settled && held) notifyError({ code: "busy_open", id, detail: `a menu, dialog or question was still open after ${budget} ms` }, what, name);
    else notifyError({ code: "busy_edits", id, detail: `the edits made before the press were not saved within ${budget} ms` }, what, name);
  }

  /** Whether the last review answer became a new undo step (`runReview`), for the tag of its Undo (UX-1601). */
  let reviewStepPushed = false;
  /**
   * Phase 2 of `reviewTransaction`: claims the transaction at once
   * (synchronously) and sends the call. `place`: where the answer goes in
   * the undo history, a new step when not given.
   */
  function runReview(what: ReviewPress, name: string, call: () => Promise<Project>, after: (p: Project) => void | Promise<void>, place?: () => void) {
    reviewWhat = what;
    return reviewing.run(async () => {
      try {
        clearTimeout(saveTimer);
        // A read of the file that runs across this write may be older than
        // it: it is read again instead of passing for an outside change
        // (as for the editor's own saves).
        saveGen++;
        const base = lastSaved;
        answerEntry = null;
        // Reads of the review from before the call are of the job before
        // it: dropped, the read after it is the one shown (ERR-1905).
        activityReads.bump();
        const p = await call().finally(() => {
          saveGen++;
          activityReads.bump();
        });
        if (destroyed) return;
        await drag.settled();
        applyCoreAnswer(base, p, answerEntry);
        recalcFor = p.result_snapshot?.taken_ms ?? null;
        // An editor step: Undo brings the job before it back (UI-802), for a
        // revert through the core, logged (`undoVia`, UX-1601).
        if (place) {
          reviewStepPushed = false;
          place();
        } else {
          reviewStepPushed = pushHistory();
        }
        closeStaleDialogs();
        recalc();
        await after(p);
        await refreshActivity();
      } catch (e) {
        // Told also when the editor was left meanwhile (a teardown that does
        // not ask, the folder switched): the notice outlives it and names
        // the job (ERR-1105).
        // `busy`: another writer held the job's lock, nothing changed.
        // `revert_stale`: the job is another one by now. The list is read again.
        notifyError(e, what, name);
        if (!destroyed) await refreshActivity();
      }
    });
  }

  /** Puts the offer's own figures back in place of the recalculation logged at `entryTs`. */
  function restoreOfferNow(entryTs: number) {
    return reviewTransaction(
      "restoreOffer",
      async () => {
        const p = await api.restoreOfferResult(id, entryTs);
        // The put-back's own entry, newest when nothing came after it: the
        // job the core answered has its effect (`seenTs`, design B).
        const log = await api.projectActivity(id).catch(() => [] as ActivityItem[]);
        const own = log[0];
        if (own && own.restores === entryTs && !isAi(own.actor)) answerEntry = own.ts_ms;
        return p;
      },
      () => {},
    );
  }

  /**
   * Accept: the banner stays (its buttons busy) until the core answered;
   * a `busy` answer keeps the AI's changes pending and says so under
   * Accept's own title (ERR-1103). It accepts everything the editor has
   * shown (`seenTs`), also the AI's own reverts that are no listed step,
   * so one Accept ends the review (audit 2026-11-24 ERR-1801), and at least
   * up to the newest step of the review shown (TEST-2012).
   */
  async function acceptNow() {
    if (!pending) return;
    // Also up to the newest step the banner's review lists (TEST-2012).
    const ts = acceptTs(seenTs, pending.ts_ms, review?.steps);
    const name = project?.name.trim() || id;
    // A read of the review on its way from before the Accept never brings
    // the banner back after it (ERR-1905).
    activityReads.bump();
    try {
      await api.acceptChanges(id, ts);
      activityReads.bump();
      if (destroyed) return;
      review = null;
      focusChange = null;
    } catch (e) {
      notifyError(e, "accept", name);
    }
    if (!destroyed) await refreshActivity();
  }

  async function revertNow(all: boolean, expectTs?: number) {
    // Reverting several AI steps at once is one click next to Accept, and
    // after a reopen it cannot be undone: ask first, naming the steps (UX-011).
    // Revert all goes back to the job as it was right after the AI created
    // it: the creation itself has no version before it (HUNT-1601), so it
    // is neither counted nor promised away (ERR-1705).
    const steps = pendingList.filter((a) => !a.created);
    const createdFirst = !!pendingList[pendingList.length - 1]?.created;
    const shown = steps.length;
    // The review the banner showed: the core refuses (`revert_stale`) when
    // it is no longer open, so a banner a slow read brought back after
    // Accept takes nothing accepted out (ERR-1905).
    const shownSince = review?.since_ts_ms;
    let staleReview = false;
    if (all && shown > 1) {
      // Tauri's dialog plugin makes confirm asynchronous: always await it.
      if (!(await confirm(t(createdFirst ? "review.confirmRevertAllCreated" : "review.confirmRevertAll", { n: shown })))) return;
    }
    // AI steps that came in after the banner was drawn (while the question
    // was open, while the press waited for the edits before it or for the
    // job's lock) are reverted too, and said so with the count the core
    // took under the lock (DR-003; HUNT-1205, round 13 design E), also when
    // the banner showed one change. Only the AI's entries count, as in the
    // banner: a user's revert from a second Tectora is no AI step (HUNT-1401).
    let undone = 0;
    // The entry the AI tab's Revert was drawn on, named when it is undone.
    const entry = all || expectTs == null ? undefined : activity.find((a) => a.ts_ms === expectTs);
    const entryWords = entry ? entryText(entry, activity) : "";
    // The revert wrote over a file another program made unreadable (ERR-1502).
    let repaired = false;
    // The revert entry the core logged: Undo takes exactly it back (UX-1601).
    let logged: number | null = null;
    // Edits not on disk yet would be lost: the store keeps only the edits in
    // the file, merging the revert onto them (ERR-001); the transaction
    // saves them first.
    await reviewTransaction(
      all && shown > 1 ? "revertAll" : "revert",
      async () => {
        const oldest = steps[steps.length - 1];
        if (all && oldest) {
          // The review the banner showed (`since_ts_ms`): the core writes the
          // job as the user last settled it, or answers `revert_stale` when
          // that review is over by now (ADR 0007, ERR-1905); the editor then
          // says so and reads the job again. An AI's new job keeps its creation.
          const r = await (review ? api.revertReview(id, review.since_ts_ms) : api.revertAll(id, oldest.ts_ms)).catch((e) => {
            if (shownSince == null || toAppError(e).code !== "revert_stale") throw e;
            staleReview = true;
            throw { code: "review_stale", id, detail: toAppError(e).detail };
          });
          undone = r.undone_ai;
          repaired = !!r.repaired;
          logged = r.entry_ts_ms ?? null;
          answerEntry = logged;
          return r.project;
        }
        // The latest change is undone only if it still is the entry the button was drawn on (ERR-801).
        const r = await api.revertLastChange(id, expectTs);
        repaired = !!r.repaired;
        logged = r.entry_ts_ms ?? null;
        answerEntry = logged;
        return r.project;
      },
      () => {
        focusChange = null;
        if (logged !== null && reviewStepPushed) undoVia[at] = { take: logged };
        if (repaired || outsideUnreadable) outsideOver(repaired);
        // Nothing written: the job already is as before the changes (ERR-1601).
        if (logged === null) info(t("review.nothingToRevert"));
        else if (all && undone > shown) info(t("review.revertedMore", { n: undone - shown }));
        // The one change undone, by name: the banner may stay for the others (UX-1501).
        else if (entry) info(t("ai.revertedOne", { s: entryWords }));
      },
    );
    // The review shown is gone (accepted or reverted meanwhile): nothing was
    // reverted, the note said so; the job and its review are read again.
    if (staleReview && !destroyed) void load(false);
  }

  /**
   * The dialog's part (or dormer) is still the one it was opened for. An
   * outside save (AI, CLI) that removed or changed it closes the dialog
   * with a note instead of acting on another part (ERRU-010).
   */
  function dialogPartThere(which: "calc" | "xform" | "round"): boolean {
    if (dialogHolds[which] && !dialogHolds[which]!.gone) return true;
    closeStaleDialogs();
    return false;
  }

  /**
   * Closes the dialogs whose part or dormer an outside save removed or
   * changed (ERRU-010), and moves the others with their part when an earlier
   * one was removed. A dialog whose OK is under way is closed without a
   * second note: its edit says that it was not made.
   */
  function closeStaleDialogs() {
    let closed = false;
    const gone = (h: Hold | undefined, quiet: boolean) => {
      if (h && !h.gone) return false;
      if (!quiet) closed = true;
      return true;
    };
    if (calc) gone(dialogHolds.calc, acting.calc) ? (calc = null) : (calc.part = dialogHolds.calc!.index);
    if (xform) gone(dialogHolds.xform, acting.transform) ? (xform = null) : (xform.part = dialogHolds.xform!.index);
    if (round) gone(dialogHolds.round, acting.round) ? (round = null) : (round.part = dialogHolds.round!.index);
    if (dormerDlg && dormerDlg.index !== null) gone(dialogHolds.dormer, acting.dormer) ? (dormerDlg = null) : (dormerDlg.index = dialogHolds.dormer!.index);
    if (closed) info(t("editor.dialogStale"));
  }

  /** Opens the calculation helper for wall `edge` of part `part`. */
  function openCalc(part: number, edge: number) {
    dialogHolds.calc = jobEdits.hold("parts", part);
    calc = { part, edge, request: null };
  }

  // Calculation helpers on the selected wall.
  let calc = $state<{ part: number; edge: number; request: HelperRequest | null } | null>(null);

  /** The move / rotate / mirror / copy dialog for the selected part (DACAD: Bearbeiten). */
  let xform = $state<{ part: number; edge: number | null; mode: TransformMode; op: TransformOp | null } | null>(null);

  function openTransform() {
    if (!project || !selection) return;
    dialogHolds.xform = jobEdits.hold("parts", selection.part);
    xform = { part: selection.part, edge: selection.edge, mode: "move", op: null };
  }

  /** The dialog's OK: the core moves, copies or shifts (the MCP tool transform_part / the wall shift). */
  const applyTransform = exclusive(
    () => pendingEdits.track(applyTransformNow()),
    (b) => (acting.transform = b),
  );
  async function applyTransformNow() {
    if (!project || !xform?.op) return;
    if (!dialogPartThere("xform")) return;
    const op = xform.op;
    try {
      // Onto the part the dialog was opened for, on the job as it is then (ERR-203).
      const o = await jobEdits.run(
        dialogHolds.xform!,
        async (job, pi) =>
          op.kind === "wall"
            ? { project: await api.offsetWall(plain(job), pi, op.edge, op.dist), parts: [] as number[] }
            : await api.transformPart(plain(job), pi, op.map, op.copies),
        ({ project: next, parts }) => {
          adoptProject(next);
          if (op.kind !== "wall" && parts.length) selection = { part: parts[parts.length - 1], edge: 0 };
          xform = null;
          commit();
        },
      );
      if (o === "gone") xform = null;
      if (o !== "done") return;
    } catch (e) {
      if (!notifyRefusal(e, "transform")) notifyError(e, "calc");
      return;
    }
    setTimeout(() => plan?.fit(), 0);
  }

  /** Takes what the core returned for an edit of the parts and the things on them. */
  function adoptProject(next: Project) {
    if (!project) return;
    project.parts = next.parts;
    project.dormers = next.dormers;
    project.chimneys = next.chimneys;
    if (next.windows) project.windows = next.windows;
    if (next.roof_openings) project.roof_openings = next.roof_openings;
    if (next.manual_lines) project.manual_lines = next.manual_lines;
    if (next.manual_faces) project.manual_faces = next.manual_faces;
  }

  /** The dialog's OK: the core applies the helper's values (the MCP tool roof_helper with apply). */
  const applyCalc = exclusive(
    async () => {
      if (!project || !calc?.request) return;
      if (!dialogPartThere("calc")) return;
      // Open with its values until the edit is made: a refused one is typed again otherwise.
      if ((await pendingEdits.track(applyHelper(dialogHolds.calc!, calc.edge, calc.request))) === "done") calc = null;
    },
    (b) => (acting.calc = b),
  );

  /**
   * Applies a helper request to wall `edge` of the held part through the
   * core, onto the same part; resolves to how the edit ended ("refused":
   * the core refused it or it failed, said in a toast).
   */
  async function applyHelper(hold: Hold, edge: number, request: HelperRequest): Promise<Outcome | "refused"> {
    if (!project) return "closed";
    try {
      return await jobEdits.run(
        hold,
        (job, part) => api.roofHelperApply(plain(job), part, edge, request),
        (next) => {
          if (!project) return;
          project.parts = next.parts;
          commit();
        },
      );
    } catch (e) {
      if (!notifyRefusal(e, "helper")) notifyError(e, "calc");
      return "refused";
    }
  }

  async function openRound() {
    if (!project || !selection) return;
    const { part, edge } = selection;
    let c;
    try {
      c = await api.roofHelperContext($state.snapshot(project) as Project, part, edge);
    } catch (e) {
      notifyError(e, "calc");
      return;
    }
    if (!project || c.opposite === null) return;
    const e = project.parts[part].edges[edge];
    dialogHolds.round = jobEdits.hold("parts", part);
    round = {
      part,
      a: edge,
      b: c.opposite,
      span: c.span,
      eave: e.eave_height,
      overhang: e.overhang,
      arc: null,
      request: null,
    };
  }

  /** Sets both walls to the arc's pitch and breaks (DACAD: Runddach), as the core works them out. */
  const applyRound = exclusive(
    async () => {
      if (!project || !round?.arc || !round.request) return;
      if (!dialogPartThere("round")) return;
      const r = round;
      // Open with its values until the edit is made (ERR To check 55).
      if ((await pendingEdits.track(applyHelper(dialogHolds.round!, r.a, r.request!))) === "done" && round === r) round = null;
    },
    (b) => (acting.round = b),
  );

  /** Makes the current result the job's stored result (the offer's figures). */
  const storeResult = exclusive(
    () => pendingEdits.track(storeResultNow()),
    (b) => (acting.store = b),
  );
  async function storeResultNow() {
    // Zero figures of a roof the core refused are no offer (DR-001).
    if (blockedReason) return;
    // A value typed just before is part of the roof: wait for it.
    await jobEdits.idle();
    if (!project) return;
    const roof = jobEdits.holdRoof();
    try {
      const snap = await api.takeSnapshot(JSON.parse(JSON.stringify(project)));
      await drag.settled();
      // Figures of the roof as it was asked for, never of one an AI save or
      // an edit changed meanwhile (ERR-102); an AI save of the notes keeps it (ERR-301).
      if (!project || !jobEdits.roofSame(roof)) {
        info(t("snap.stale"));
        return;
      }
      project.result_snapshot = snap;
      commit();
    } catch (e) {
      // A refusal (no building, no roof), not a failed save of the job (UI-206).
      notifyError(e, "storeResult");
    }
  }

  /**
   * A wall's eave height is not the building height: before an eave edit a
   * building without a height of its own keeps the one it shows (core
   * `RoofPart::freeze_height`, MCP update_edges does the same).
   */
  function freezeHeight(part: RoofPart, patch: Partial<EdgeDef>) {
    if ("eave_height" in patch && part.height == null) part.height = buildingHeight(part);
  }

  function patchEdge(patch: Partial<EdgeDef>) {
    if (!project || !selection) return;
    freezeHeight(project.parts[selection.part], patch);
    Object.assign(project.parts[selection.part].edges[selection.edge], patch);
    commit();
  }

  function applyAll(patch: Partial<EdgeDef>) {
    if (!project || !selection) return;
    freezeHeight(project.parts[selection.part], patch);
    for (const e of project.parts[selection.part].edges) {
      if ("pitch_deg" in patch && (isGable(e) || isMarker(e))) continue;
      Object.assign(e, patch);
    }
    commit();
  }

  /**
   * The bar's "Building height": the selected part at `h` (top of the
   * walls); its eaves, breaks, half-hips and the objects on it move by the
   * change, so a lower eave stays lower. The core's operation, the same as
   * MCP set_part_height.
   */
  const setPartHeight = (h: number) => {
    if (!project || !selection) return;
    // The part the height is typed for, held now (jobedits.ts).
    return pendingEdits.track(setPartHeightNow(jobEdits.hold("parts", selection.part), h));
  };
  /** Resolves to the reason when the height was not set, so the field shows the stored one again. */
  async function setPartHeightNow(hold: Hold, h: number): Promise<string | undefined> {
    try {
      return notMade(
        await jobEdits.run(
          hold,
          (job, pi) => api.setPartHeight(plain(job), pi, h),
          (next) => {
            if (!project) return;
            project.parts = next.parts;
            project.dormers = next.dormers;
            project.chimneys = next.chimneys;
            if (project.manual_lines?.length) project.manual_lines = next.manual_lines ?? [];
            commit();
          },
        ),
      );
    } catch (e) {
      // A height the core refuses (an eave below the ground) is a refusal
      // with its reason, and the field goes back to the stored height (ERR-003).
      const why = notifyRefusal(e, "height");
      if (why) return why;
      notifyError(e, "calc");
      return t("num.refused");
    }
  }

  /**
   * The dimension fields of a dormer, chimney, window or opening: moves it
   * to the typed distance from the eave or a side of its roof face (the
   * core's operation, the same as MCP place_item).
   */
  const placeItem = (it: ItemRef, fromEave: number | null, fromLeft: number | null, fromRight: number | null) => {
    // The object the distance is typed for, held now (jobedits.ts).
    return pendingEdits.track(placeItemNow(it.kind, jobEdits.hold(ITEM_LIST[it.kind], it.index), fromEave, fromLeft, fromRight));
  };
  async function placeItemNow(kind: ItemRef["kind"], hold: Hold, fromEave: number | null, fromLeft: number | null, fromRight: number | null): Promise<string | undefined> {
    try {
      return notMade(
        await jobEdits.run(
          hold,
          (job, index) => api.placeItem(plain(job), MCP_KIND[kind], index, fromEave, fromLeft, fromRight),
          (next) => {
            if (!project) return;
            project.dormers = next.dormers;
            project.chimneys = next.chimneys;
            project.windows = next.windows;
            if (next.roof_openings) project.roof_openings = next.roof_openings;
            commit();
          },
        ),
      );
    } catch (e) {
      // A distance that would take the object off its face is refused: said
      // in the user's language, and the field shows where it still is.
      const why = notifyRefusal(e, "place");
      if (!why) notifyError(e, "calc");
      recalc();
      return why ?? t("num.refused");
    }
  }

  /** Changes an edge length by moving its end vertex along the edge. */
  /**
   * A typed wall length: the core moves the end corner and the corners after
   * it along the wall, so the angles stay and a rectangle stays a rectangle
   * (UX-005, the MCP tool's `set_length`). Resolves to false when refused.
   */
  const setLength = (len: number) => {
    if (!project || !selection || !project.parts[selection.part]) return false;
    // The part the length is typed for, held now (jobedits.ts).
    return pendingEdits.track(setLengthNow(jobEdits.hold("parts", selection.part), selection.edge, len));
  };
  async function setLengthNow(hold: Hold, edge: number, len: number): Promise<boolean | string> {
    try {
      const o = await jobEdits.run(
        hold,
        (job, part) => api.setWallLength($state.snapshot(job.parts[part]) as RoofPart, edge, len),
        (next, part) => {
          if (!project?.parts[part]) return;
          project.parts[part].edges = next.edges;
          commit();
        },
      );
      return notMade(o) ?? true;
    } catch (e) {
      notifyError(e, "calc");
      return false;
    }
  }

  function deleteVertex() {
    if (!project || !selection) return;
    deleteVertexAt(selection.part, selection.edge);
  }

  /** Deletes corner `edge` of part `part` (a marker, or a corner of a building with more than three). */
  function deleteVertexAt(part: number, edge: number) {
    const es = project?.parts[part]?.edges;
    if (!es?.[edge] || (!isMarker(es[edge]) && es.filter((e) => !isMarker(e)).length <= 3)) return;
    es.splice(edge, 1);
    selection = null;
    commit();
  }

  /**
   * Delete / Backspace with a wall selected: the key is easy to hit while
   * stepping through walls (Backspace meant for a field), and it changes the
   * footprint, so it asks first (UX-006). The bar's trash button does not.
   */
  async function deleteVertexByKey() {
    // One question and its edit: the prepare phase of Revert waits for both (HUNT-1204).
    return asking(async () => {
      if (!project || !selection) return;
      const sel = selection;
      const part = project.parts[sel.part];
      const es = part?.edges;
      if (!es?.[sel.edge] || (!isMarker(es[sel.edge]) && es.filter((e) => !isMarker(e)).length <= 3)) return;
      const msg = isMarker(es[sel.edge]) ? t("edge.confirmDeleteMarker") : t("edge.confirmDeleteVertex", { n: sel.edge + 1, name: partLabel(part.name, sel.part) });
      // The building asked about is held (jobedits.ts): an AI save or the
      // user's own edit landing while the question is open moves it or, when
      // the save changed or removed it, refuses with a note (UI-401).
      const hold = jobEdits.hold("parts", sel.part);
      // Tauri's dialog plugin makes confirm asynchronous: always await it.
      if (!(await confirm(msg))) return;
      if (!project) return;
      if (hold.gone) {
        if (!hold.removed) notMadeNote("gone", hold);
        return;
      }
      deleteVertexAt(hold.index, sel.edge);
    });
  }

  /** A corner's coordinates typed in the wall bar: the corner moves there, as a drop would put it. */
  function setCorner(part: number, v: number, x: number, y: number) {
    if (!project || inputClosed()) return;
    const es = project.parts[part]?.edges;
    if (!es?.[v] || !Number.isFinite(x) || !Number.isFinite(y)) return;
    const p = es[v].start;
    if (p.x === x && p.y === y) return;
    // No answer of the core lands between the move and its merge (UI-502).
    const resume = jobEdits.pause();
    moveCorner(es, v, x, y);
    recalc();
    return pendingEdits.track(endVertexDragNow(part, v).finally(resume));
  }

  /** End of a corner move: a corner dropped onto its neighbour merges into it, then the step is recorded. */
  async function endVertexDragNow(part: number, v: number) {
    const was = project?.parts[part];
    if (!project || !was) return;
    const drawn = JSON.stringify($state.snapshot(was));
    try {
      const r = await api.mergeDroppedCorner($state.snapshot(was) as RoofPart, v);
      // Only onto the part as it was dropped: an edit or an AI save in between wins.
      if (r.merged && project.parts[part] && JSON.stringify($state.snapshot(project.parts[part])) === drawn) {
        project.parts[part] = r.part;
        if (selection?.part === part) selection = { part, edge: Math.min(v, r.part.edges.length - 1) };
      }
    } catch (e) {
      notifyError(e, "calc");
    }
    drag.write(commit);
  }

  function insertVertex(part: number, edge: number, x: number, y: number) {
    if (!project || inputClosed()) return;
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
    const ang = defaultDirection(es, selection.edge);
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
    dormerBlocked = null;
    dormerNotice = null;
    dormerDlg = { index: null, params: defaultParams("gable", { x, y }, angle, eave) };
  }

  function editDormer() {
    if (!project || dormerSel === null) return;
    const p = parseDormer(project.dormers[dormerSel]);
    dormerBlocked = null;
    dormerNotice = null;
    if (p.kind === "general") return;
    dialogHolds.dormer = jobEdits.hold("dormers", dormerSel);
    dormerDlg = { index: dormerSel, params: p };
  }

  const applyDormerDialog = exclusive(applyDormerDialogNow, (b) => (acting.dormer = b));
  async function applyDormerDialogNow() {
    // A distance typed into the dialog commits on blur and comes back from
    // the core: OK takes it once it is there (review UI-001).
    await tick();
    await pendingEdits.settled();
    await tick();
    if (!project || !dormerDlg) return;
    // A value the core refused, or a front on no roof face: the dialog stays
    // open with the reason next to OK (UI-102, UX-109).
    if (dormerBlocked) return;
    // The core checks the dormer as it stands once more (UI-203): a refusal
    // stays in the dialog with its reason next to OK.
    if (dormerDialog && !(await dormerDialog.finalCheck())) return;
    await tick();
    if (!project || !dormerDlg || dormerBlocked) return;
    if (dormerDlg.index !== null && dialogHolds.dormer?.gone !== false) {
      dormerDlg = null;
      info(t("editor.dialogStale"));
      return;
    }
    const d = buildDormer($state.snapshot(dormerDlg.params) as DormerParams);
    const i = dormerDlg.index === null ? null : dialogHolds.dormer!.index;
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
      closeFace();
      return;
    }
    manualDraft = [...manualDraft, p];
  }

  /**
   * Closes the face being drawn (a click on its first corner, Enter or the
   * bar's Close, UX-010). The core checks it first: corners clicked out of
   * order (an outline that crosses itself) are refused with a note and the
   * draft stays open to be finished again (HUNT-1801).
   */
  const closeFace = () => pendingEdits.track(closeFaceNow());
  async function closeFaceNow() {
    if (!project || placing !== "face" || manualDraft.length < 3) return;
    const drawn = JSON.stringify($state.snapshot(manualDraft));
    // DACAD asks for the pitch with 30° filled in.
    const face: ManualFace = { points: manualDraft.map((q) => ({ x: q[0], y: q[1] })), pitch_deg: 30 };
    let refused: Awaited<ReturnType<typeof api.checkFace>>;
    try {
      refused = await api.checkFace(face);
    } catch (e) {
      notifyError(e, "calc");
      return;
    }
    await drag.settled();
    // Drawing went on or was left while the core answered: this close is stale.
    if (!project || placing !== "face" || JSON.stringify($state.snapshot(manualDraft)) !== drawn) return;
    // Revert or the put-back went out meanwhile: the face stays open (as finishDraw, ERR-1101).
    if (reviewing.active) return;
    if (refused) {
      info(t(`manual.refused.${refused}`));
      return;
    }
    project.manual_faces ??= [];
    project.manual_faces.push(face);
    manualDraft = [];
    placing = false;
    selectManual({ kind: "face", index: project.manual_faces.length - 1 });
    commit();
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
  const faceToPart = exclusive(
    () => pendingEdits.track(faceToPartNow()),
    (b) => (acting.face = b),
  );
  async function faceToPartNow() {
    if (!project || manualSel?.kind !== "face" || !project.manual_faces) return;
    const face = project.manual_faces[manualSel.index];
    if (!face || face.points.length < 3) return;
    try {
      // Onto the face it was asked for, on the job as it is then (ERR-203):
      // the new building's number and place come from that job and the
      // core's answer, never from the job before the await (ERR-307).
      await jobEdits.run(
        jobEdits.hold("manual_faces", manualSel.index),
        (job, index) => {
          const n = job.parts.length;
          const name = n === 0 ? t("editor.mainPart") : t("editor.partN", { n: n + 1 });
          return api.faceToPart(plain(job), index, name, presets.pitch_deg, presets.eave_height, presets.overhang);
        },
        (next) => {
          if (!project) return;
          project.manual_faces = next.manual_faces;
          project.parts = next.parts;
          manualSel = null;
          // The core adds the new building after the others.
          selection = { part: next.parts.length - 1, edge: 0 };
          commit();
        },
      );
    } catch (e) {
      if (!notifyRefusal(e, "face")) notifyError(e, "calc");
    }
  }

  function deleteManual() {
    if (!project || !manualSel) return;
    if (manualSel.kind === "line") project.manual_lines?.splice(manualSel.index, 1);
    else project.manual_faces?.splice(manualSel.index, 1);
    manualSel = null;
    commit();
  }

  /** Places a roof opening with its first corner at the click (DACAD: Öffnung). */
  const placeHole = (x: number, y: number) => pendingEdits.track(placeHoleNow(x, y));
  async function placeHoleNow(x: number, y: number) {
    if (!project) return;
    placing = false;
    // 1 × 1 m square along the nearest wall [no source: placeholder size].
    const angle = nearestWallAngle(project.parts, { x, y });
    let hole: Chimney;
    try {
      hole = await api.chimneyFromRect({ x, y, width: 1, depth: 1, rotation: +((angle * 180) / Math.PI).toFixed(2), height: 0 });
    } catch (e) {
      notifyError(e, "calc");
      return;
    }
    await drag.settled();
    if (!project) return;
    project.roof_openings ??= [];
    project.roof_openings.push(hole);
    selectOpening({ kind: "hole", index: project.roof_openings.length - 1 });
    placedOn("opening", project.roof_openings.length - 1);
    commit();
  }

  /** Places a chimney with its first corner at the click (DACAD: Schornstein). */
  const placeChimney = (x: number, y: number) => pendingEdits.track(placeChimneyNow(x, y));
  async function placeChimneyNow(x: number, y: number) {
    if (!project) return;
    placing = false;
    // 0.6 m square, top 1 m above the roof at its middle [no source: common
    // size]. The middle of the turned square, not x + 0.3: turned along a
    // wall it can lie on the other side of the click (MCP add_object, core
    // `probe::chimney_default_base`).
    const angle = nearestWallAngle(project.parts, { x, y });
    let chimney: Chimney;
    try {
      chimney = await api.chimneyFromRect({ x, y, width: 0.6, depth: 0.6, rotation: +((angle * 180) / Math.PI).toFixed(2), height: 0 });
      const c = chimney.corners;
      const mid = { x: c.reduce((a, p) => a + p.x, 0) / c.length, y: c.reduce((a, p) => a + p.y, 0) / c.length };
      chimney.height = roundTo((roofHeightAt(result, mid) ?? presets.eave_height) + 1, 2);
    } catch (e) {
      notifyError(e, "calc");
      return;
    }
    await drag.settled();
    if (!project) return;
    project.chimneys.push(chimney);
    selectOpening({ kind: "chimney", index: project.chimneys.length - 1 });
    placedOn("chimney", project.chimneys.length - 1);
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
    placedOn("window", project.windows.length - 1);
    commit();
  }

  function selectOpening(s: OpeningSel) {
    selection = null;
    dormerSel = null;
    manualSel = null;
    openSel = s;
  }

  /** Stores `c` as the chimney or roof opening `at` (the one the value was typed for, even when ▶ moved the selection since, UI-202). */
  function setChimney(c: Chimney, at: NonNullable<OpeningSel>) {
    if (!project || at.kind === "window") return;
    if (at.kind === "chimney") {
      if (!project.chimneys[at.index]) return;
      project.chimneys[at.index] = c;
    } else {
      if (!project.roof_openings?.[at.index]) return;
      project.roof_openings[at.index] = c;
    }
    commit();
  }

  /** A field of roof window `at` typed (`patch`); resolves to the reason when it was not stored. */
  const setWindow = (patch: Partial<RoofWindow>, at: number) => pendingEdits.track(setWindowNow(patch, jobEdits.hold("windows", at)));
  async function setWindowNow(patch: Partial<RoofWindow>, hold: Hold): Promise<string | undefined> {
    try {
      return notMade(
        await jobEdits.run(
          hold,
          async (job, index) => {
            // Only the field typed is the user's; the rest is the window as it is then.
            const w = { ...($state.snapshot(job.windows![index]) as RoofWindow), ...patch };
            // A window larger than its face would make the face area negative
            // (MATH-001): refused with the reason, as MCP set_openings refuses it.
            const big = await api.checkWindows({ ...plain(job), windows: job.windows!.map((x, i) => (i === index ? w : $state.snapshot(x))) } as Project);
            if (big) throw { tooLarge: big };
            return w;
          },
          (w, index) => {
            if (!project?.windows?.[index]) return;
            project.windows[index] = w;
            commit();
          },
        ),
      );
    } catch (e) {
      const big = (e as { tooLarge?: { index: number; area: number; face_area: number } } | null)?.tooLarge;
      if (!big) {
        notifyError(e, "calc");
        return t("num.refused");
      }
      notify({
        kind: "error",
        code: "window_too_large",
        title: t("opening.tooLarge.title"),
        message: t("opening.tooLarge", { n: big.index + 1, a: num(big.area), f: num(big.face_area) }),
      });
      // The field shows the stored size again.
      return t("opening.tooLarge.title");
    }
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

  /** The object dialog of a chimney, roof window or opening (Properties, double-click). */
  let objDlg = $state(false);
  /** The undo step when the object dialog opened: Cancel goes back to it. */
  let objDlgAt = 0;
  $effect(() => {
    if (!openSel) objDlg = false;
  });

  /** OK keeps the dialog's edits; Cancel, Escape and × undo them, as the
      dormer dialog does (review UX-004). */
  async function closeObjDlg(keep: boolean) {
    objDlg = false;
    if (keep) return;
    // A typed value commits on blur and a placement comes back from the core.
    await tick();
    await pendingEdits.settled();
    if (!project || at <= objDlgAt || !history[objDlgAt]) return;
    const sel = openSel;
    // Back to the step the dialog opened on, and the cancelled edits out of
    // the history, so Undo does not bring them back.
    // Refused (a core action is out): the history stays whole, Undo works.
    if (!restore(objDlgAt)) return;
    history = history.slice(0, objDlgAt + 1);
    historyLen = history.length;
    openSel = sel && project && itemExists(project, sel) ? sel : null;
  }

  /**
   * Properties of an object, from a double-click or the context menu: the
   * dormer window for a dormer, the object dialog for a chimney, window or
   * opening, the bar's first field for a hand-drawn line or face.
   */
  function openProperties(it: ItemRef) {
    if (!project || inputClosed() || !itemExists(project, it)) return;
    selectItem(it);
    if (it.kind === "dormer") editDormer();
    else if (it.kind === "chimney" || it.kind === "window" || it.kind === "hole") {
      objDlgAt = at;
      objDlg = true;
    }
    else focusProperties();
  }

  /**
   * The drag under way in the plan (dragsession.ts; jobedits.ts rule 7): one
   * transaction from the press until its result is in the history, and the
   * only writer of the job meanwhile. An object or a building is rebuilt
   * from the job of the press at every step; a corner is moved in place.
   * A step's `to` is (dx, dy) from the press, for a corner its new place.
   */
  const drag = new DragSession<PlanTarget, Point2>(
    {
      pause: () => jobEdits.pause(),
      hold: (target) =>
        !project ? null
        : target.type === "item" ? jobEdits.hold(ITEM_LIST[target.item.kind], target.item.index)
        : target.type === "part" || target.type === "vertex" ? jobEdits.hold("parts", target.part)
        : null,
      snapshot,
      step: dragStep,
      drop: dragDrop,
      restore: (base) => {
        const sel = selection;
        project = JSON.parse(base) as Project;
        selection = keptSelection(project, sel);
        recalc();
      },
      changed: (state) => {
        if (state === "armed") dragStart = { result, from: null };
        dragState = state;
        // A read that met a save of ours while the drag was open (the
        // outside save itself waits in `drag.defer`).
        if (state === "idle" && reloadWanted && !inFlight) {
          reloadWanted = false;
          load(false);
        }
      },
    },
    // The dev build (and the E2E run on it) fails loudly on a write that
    // slipped past the guards; a release ends the drag where the job is.
    import.meta.env.DEV,
  );
  /** The session's state, for what the editor shows and closes. */
  let dragState = $state<DragState>("idle");
  /** The roof and the object's centre when the drag was pressed (the drop's roof follow maps from them). */
  let dragStart: { result: CalcResult | null; from: Point2 | null } = { result: null, from: null };
  /** A drag is open: pressed, moving, or its drop still with the core. */
  const dragging = () => drag.active;
  /**
   * Revert, Revert all or the put-back of the offer's figures is with the
   * core: like a drag, the only writer of the job until its answer is in
   * (dragsession.ts `CoreAction`, design A1).
   */
  const reviewing = new CoreAction(
    () => jobEdits.pause(),
    (on) => (reviewOut = on),
    import.meta.env.DEV,
  );
  let reviewOut = $state(false);
  /** Which review action is out, for the status bar's word (UX-1102). */
  let reviewWhat = $state<ReviewPress>("revert");
  /** The user's input is refused now: a drag is open or a review action is out (one predicate for every refusal). */
  const inputClosed = () => closedForInput(drag, reviewing);
  /**
   * The editor is closed for the user's input: a drag is the only writer
   * of the job until its result is in the history. Bars, toolbar, dock and
   * header are `inert`, keys are refused (Escape cancels, F3 passes; Undo
   * and Redo wait for a drop that settles), the plan takes no new press.
   * A review action closes the plan too: nothing in it is the action's.
   */
  const closed = $derived(dragState !== "idle" || reviewOut);

  /** A press in the plan that may become a drag: false when a drag is open already. */
  function armDrag(target: PlanTarget, pointerId: number): boolean {
    if (!project || reviewing.active) return false;
    const ok = drag.arm(pointerId, target);
    if (ok && target.type === "item") dragStart.from = itemCentre(project, target.item);
    return ok;
  }

  /** A step of an object's or a building's drag. */
  function dragObject(target: PlanTarget, dx: number, dy: number) {
    if (target.type !== "item" && target.type !== "part") return;
    if (!Number.isFinite(dx) || !Number.isFinite(dy)) return;
    drag.step({ x: dx, y: dy });
  }

  /** A step of a corner's drag. */
  function moveVertex(_part: number, _v: number, x: number, y: number) {
    if (!Number.isFinite(x) || !Number.isFinite(y)) return;
    drag.step({ x, y });
  }

  /** The press ended (release, lost pointer, the plan went away): the drop, or nothing when it never moved. */
  function endDragPress() {
    const done = pendingEdits.track(drag.drop());
    // A refusal of the drop was shown where it was caught.
    done.catch(() => {});
  }

  /** Escape during a drag: the job goes back to where the drag started, nothing is saved. */
  function cancelDrag() {
    drag.cancel();
  }

  /**
   * Ends the drag under way where it is now, as its release would (leaving
   * the job must not wait for a drag that only the release ends, ERR-501);
   * the plan's further steps of it find no drag and do nothing.
   */
  function cutDrag() {
    const done = pendingEdits.track(drag.cut());
    done.catch(() => {});
  }

  /** One drag step; false when it changed nothing. */
  function dragStep(d: Drag<PlanTarget, Point2>, to: Point2): boolean {
    const target = d.target;
    if (!project) return false;
    if (target.type === "vertex") {
      const es = project.parts[target.part]?.edges;
      const p = es?.[target.vertex]?.start;
      if (!es || !p || (p.x === to.x && p.y === to.y)) return false;
      moveCorner(es, target.vertex, to.x, to.y);
      recalc();
      return true;
    }
    const base: Project = JSON.parse(d.base);
    if (target.type === "item") {
      const it = target.item;
      if (!itemExists(base, it)) return false;
      moveItem(base, it, to.x, to.y);
      project = base;
      selectItem(it);
    } else if (target.type === "part") {
      if (!base.parts[target.part]) return false;
      // The plan draws the moved part at once (TypeScript, presentation);
      // what is saved on release is the core's `transform_part`.
      transformPart(base, target.part, translation(to.x, to.y));
      const sel = selection;
      project = base;
      selection = sel;
    } else return false;
    recalc();
    return true;
  }

  /** The drop: an object lands and follows its roof; a building's move and a corner's merge go on with the core. */
  function dragDrop(d: Drag<PlanTarget, Point2>): void | Promise<void> {
    const target = d.target;
    const to = d.last;
    if (!project || !to) return;
    if (target.type === "vertex") return endVertexDragNow(target.part, target.vertex);
    if (target.type === "part") return finishPartDrag(d.base, target.part, to.x, to.y);
    if (target.type !== "item") return;
    const base: Project = JSON.parse(d.base);
    const it = target.item;
    if (!itemExists(base, it)) return;
    moveItem(base, it, to.x, to.y);
    const from = dragStart.from;
    if (from) followRoof(base, it, from, { x: from.x + to.x, y: from.y + to.y }, dragStart.result);
    project = base;
    selectItem(it);
    commit();
  }

  /** A part drag ended: the core's move replaces the drawn one, then it is saved; on a refusal the part goes back. */
  async function finishPartDrag(from: string, part: number, dx: number, dy: number) {
    const drawn = JSON.stringify($state.snapshot(project));
    try {
      const r = await api.transformPart(JSON.parse(from) as Project, part, { kind: "move", dx, dy }, 0);
      if (!project || JSON.stringify($state.snapshot(project)) !== drawn) return;
      drag.write(() => {
        const sel = selection;
        adoptProject(r.project);
        selection = sel;
        commit();
      });
    } catch (e) {
      if (project && JSON.stringify($state.snapshot(project)) === drawn) {
        const sel = selection;
        project = JSON.parse(from) as Project;
        selection = sel;
        recalc();
      }
      // A refused move is named as one, not as a failed calculation (ERR-701).
      if (!notifyRefusal(e, "transform")) notifyError(e, "move");
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
    const w = it.kind === "window" ? project.windows![it.index].width : pts.length ? maxOf(pts, (p) => p.x) - minOf(pts, (p) => p.x) : 0.5;
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
    return [
      { label: t("tool.transform") + "…", icon: "move", action: () => { selection = { part: pi, edge: selection?.part === pi ? selection.edge : 0 }; openTransform(); } },
      { label: t("menu.duplicatePart"), icon: "copy", action: () => {
        if (!project) return;
        // 2 m to the right of everything drawn (as a new standard roof is
        // placed), so the copy never lands on, and joins, another building
        // or a manual face (UX-010).
        void duplicatePart(jobEdits.hold("parts", pi));
      } },
      { label: t("tool.deletePart"), icon: "trash", danger: true, action: () => { selection = { part: pi, edge: 0 }; deletePart(); } },
    ];
  }

  /** A copy of the held part to the right of everything drawn, made by the core. Once per press (UI-302). */
  const duplicatePart = exclusive(
    (hold: Hold) => pendingEdits.track(duplicatePartNow(hold)),
    (b) => (acting.copy = b),
  );
  async function duplicatePartNow(hold: Hold) {
    if (!project) return;
    try {
      // A copy of the part the user picked, on the job as it is then (ERR-203).
      const o = await jobEdits.run(
        hold,
        (job, pi) => {
          // 2 m to the right of everything drawn (as a new standard roof is
          // placed), so the copy never lands on, and joins, another building
          // or a manual face (UX-010).
          const ps = job.parts[pi].edges.map((e) => e.start);
          const w = maxOf(ps, (p) => p.x) - minOf(ps, (p) => p.x);
          const xs = [...job.parts.flatMap((p) => p.edges.map((e) => e.start.x)), ...(job.manual_faces ?? []).flatMap((f) => f.points.map((p) => p.x))];
          const dx = Math.max(w, maxOf(xs) - minOf(ps, (p) => p.x)) + 2;
          return api.transformPart(plain(job), pi, { kind: "move", dx: +dx.toFixed(3), dy: 0 }, 1);
        },
        (r) => {
          adoptProject(r.project);
          if (r.parts.length) selection = { part: r.parts[0], edge: 0 };
          commit();
        },
      );
      if (o !== "done") return;
    } catch (e) {
      if (!notifyRefusal(e, "transform")) notifyError(e, "calc");
      return;
    }
    setTimeout(() => plan?.fit(), 0);
  }

  function openMenu(target: PlanTarget, client: { x: number; y: number }, world: Point2) {
    if (!project || inputClosed()) return;
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
        { label: t("menu.properties"), icon: "sliders", action: () => openProperties(it) },
        { label: t("menu.duplicate"), icon: "copy", hint: keys("Ctrl+D"), action: () => duplicateItem(it) },
        ...(turns
          ? [
              { label: t("menu.rotateLeft"), icon: "undo", action: () => turnItem(it, 90) },
              { label: t("menu.rotateRight"), icon: "redo", action: () => turnItem(it, -90) },
            ]
          : []),
        ...(it.kind === "chimney" || it.kind === "hole" ? [{ label: t("menu.alignWall"), icon: "fit", action: () => alignItem(it) }] : []),
        { sep: true },
        { label: t("menu.delete"), icon: "trash", danger: true, hint: deleteKeyHint(isMac), action: () => removeItem(it) },
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
            ? { label: t("menu.makeEave"), icon: "roof", action: () => project?.parts[part] && patchEdge({ pitch_deg: eavePitchFor(part, project.parts[part].edges, edge, presets.pitch_deg), half_hip: null }) }
            : { label: t("menu.makeGable"), icon: "house", action: () => { const w = project?.parts[part]?.edges[edge]; if (w) rememberEavePitch(part, w); patchEdge({ pitch_deg: GABLE_PITCH }); } },
          { label: t("menu.insertPoint"), icon: "plus", action: () => insertVertex(part, edge, on.x, on.y) },
          { label: t("edge.addMarker"), icon: "right", action: insertMarker },
        ]),
        ...(!marker && !gable ? [{ label: t("calc.button"), icon: "chart", action: () => openCalc(part, edge) }] : []),
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
        { label: t("menu.deletePoint"), icon: "trash", danger: true, hint: deleteKeyHint(isMac), disabled: !canDelete, action: deleteVertex },
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
        { label: t("tool.draw"), icon: "draw", hint: drawKeyHint(isMac), action: startDraw },
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
    if (placing === "line" || placing === "face") clearSelection();
  }

  function setDormer(d: Dormer, at: number) {
    if (!project || !project.dormers[at]) return;
    project.dormers[at] = d;
    commit();
  }

  /** Asks first (UI-006): one press would remove the dormer with its faces. */
  async function deleteDormer() {
    // One question and its edit: the prepare phase of Revert waits for both (HUNT-1204).
    return asking(async () => {
      if (!project || dormerSel === null) return;
      const hold = jobEdits.hold("dormers", dormerSel);
      // Tauri's dialog plugin makes confirm asynchronous: always await it.
      if (!(await confirm(t("dormer.confirmDelete", { n: dormerSel + 1 })))) return;
      if (!project) return;
      // An AI save while the question was open: the dormer asked about is
      // removed where it is now; one the save changed or removed is not (jobedits.ts).
      if (hold.gone) {
        if (!hold.removed) notMadeNote("gone", hold);
        return;
      }
      const index = hold.index;
      project.dormers.splice(index, 1);
      if (dormerSel === index) dormerSel = null;
      else if (dormerSel !== null && dormerSel > index) dormerSel--;
      commit();
    });
  }

  /** Whether anything is drawn that Clear would remove. */
  function hasDrawing(p: Project | null): boolean {
    if (!p) return false;
    return [p.parts, p.dormers, p.chimneys, p.windows, p.roof_openings, p.manual_lines, p.manual_faces].some((l) => (l?.length ?? 0) > 0);
  }

  // Removes everything drawn and leaves an empty canvas (MCP clear_roof);
  // job data and roof options stay, and Undo brings the roof back.
  async function clearRoof() {
    // One question and its edit: the prepare phase of Revert waits for both (HUNT-1204).
    return asking(async () => {
      if (!hasDrawing(project)) return;
      const roof = jobEdits.holdRoof();
      if (!(await confirm(t("editor.confirmClear")))) return;
      // An AI save changed the roof while the dialog was open: the user did
      // not see what is there now, so say so instead (ERR-203). A save of the
      // notes or the job data clears as asked (ERR-301).
      if (!project) return;
      if (!jobEdits.roofSame(roof)) {
        notMadeNote("gone", null);
        return;
      }
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
    });
  }

  async function deletePart() {
    // One question and its edit: the prepare phase of Revert waits for both (HUNT-1204).
    return asking(async () => {
      if (!project || !selection) return;
      // The last part takes the whole roof with it: that is Clear.
      if (project.parts.length === 1) return clearRoof();
      const hold = jobEdits.hold("parts", selection.part);
      // Tauri's dialog plugin replaces window.confirm with an async version
      // (a Promise is always truthy), so the answer must be awaited.
      if (!(await confirm(t("editor.confirmDeletePart", { name: partLabel(project.parts[selection.part].name, selection.part) })))) return;
      if (!project) return;
      // An AI save while the question was open: the building asked about is
      // removed where it is now; one the save changed or removed is not (jobedits.ts).
      if (hold.gone) {
        if (!hold.removed) notMadeNote("gone", hold);
        return;
      }
      project.parts.splice(hold.index, 1);
      selection = null;
      commit();
    });
  }

  // Drawing a new footprint.
  function startDraw() {
    // Already drawing: the Insert key keeps going; the toolbar button
    // (toggle) and the context menu go through cancelDraw, which asks.
    if (drawing) return;
    clearSelection();
    placing = false;
    drawing = [];
  }

  /**
   * Picking a drawing tool (Building, Manual face, Roof line) ends the
   * selection: Delete or Backspace meant for a corner must never remove a
   * wall or object still selected from before (audit 2026-11-24 UX-2002,
   * owner decision 6).
   */
  function clearSelection() {
    selection = null;
    dormerSel = null;
    openSel = null;
    manualSel = null;
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

  /** A clicked corner of the footprint being drawn; a click on the last corner again adds no wall of no length. */
  function addDrawPoint(x: number, y: number) {
    if (!drawing) return;
    const last = drawing[drawing.length - 1];
    if (last && samePoint(last, { x, y })) return;
    drawing.push({ x, y });
  }

  const finishDraw = () => pendingEdits.track(finishDrawNow());
  async function finishDrawNow() {
    if (!project || !drawing) return;
    // Two corners on one point make a wall of no length, which the roof
    // calculation cannot take (core `zero_length_wall`): a corner clicked
    // twice, or the last corner snapped onto the first one.
    const drawn = JSON.stringify($state.snapshot(drawing));
    let pts: Point2[];
    try {
      pts = await api.withoutRepeats($state.snapshot(drawing) as Point2[]);
    } catch (e) {
      notifyError(e, "calc");
      return;
    }
    await drag.settled();
    // Drawing went on or was left while the core answered: this finish is stale.
    if (!project || !drawing || JSON.stringify($state.snapshot(drawing)) !== drawn) return;
    // Revert or the put-back went out meanwhile: its answer would throw the
    // building away. The footprint stays open to be closed again (ERR-1101).
    if (reviewing.active) return;
    if (pts.length < 3) return;
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
    // Undo right after this step reopens the outline with its corners
    // instead of leaving an empty plan (UX-004).
    finished = { step: history[at], corners: JSON.parse(drawn) };
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
      const dx = xs.length ? maxOf(xs) + 2 : 0;
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
  const isMac = isMacPlatform();
  const keys = (k: string) => shortcut(k, isMac);

  /** Undo / Redo pressed in the dock or the banner during a review call reach `onkeydown`, which queues them behind an Undo through the core (UX-1802). */
  const passUndoKeys = (e: Event) => e.type === "keydown" && !!undoKey(e as KeyboardEvent);

  function onkeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    const del = e.key === "Delete" || (e.key === "Backspace" && !mod);
    // A drag is the only writer of the job from its press until its result
    // is in the history (jobedits.ts rule 7): a key that edits (Delete, an
    // undo, a copy, a nudge, Enter on a button, a typed value) would be
    // erased by the next step of the drag, or would hit another object once
    // the pressed one is gone (ERR-502, UI-603, UI-605). So every key is
    // refused, and nothing is replayed after the drop. Escape cancels the
    // drag, F3 switches the view, and Undo / Redo pressed while the drop is
    // with the core run right after it (ERR-603). Function keys stay the
    // browser's.
    if (inputClosed() && e.key !== "F3") {
      if (/^F\d+$/.test(e.key)) return;
      e.preventDefault();
      // Undo / Redo pressed while one of them is with the core run after
      // it, in order (UI-1704, round 18 design B).
      if (coreStep && undoKey(e)) {
        coreQueue.push(undoKey(e) === "redo" ? "redo" : "undo");
        return;
      }
      // Any other review action takes no key at all: nothing of the user's is replayed after it (design A1).
      if (!drag.active) return;
      if (dragState === "settling") {
        if (undoKey(e)) undoKey(e) === "redo" ? redo() : undo();
      } else if (e.key === "Escape") cancelDrag();
      return;
    }
    // The job is still being read: its toolbar is inert, and so are its
    // keys (Insert would open the footprint entry on the job once it is in,
    // HUNT-908).
    if (!project) return;
    const tag = (e.target as HTMLElement).tagName;
    // A number field with its value stored (nothing typed since Enter):
    // Ctrl+Z undoes that change, not only the field's text (UX-001).
    if ((tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") && !editorTakesUndo(e.target as HTMLElement, e)) return;
    // A dialog is open: its keys (Escape closes it) must not edit the roof behind it.
    if (document.querySelector('[role="dialog"]')) return;
    // Corners being placed: Ctrl+Z (and Backspace for a face or line)
    // removes the last one and nothing else (UX-1902, owner decision 3);
    // with none left, and for Delete, nothing (UX-2002, decision 6).
    const draftKind = drawing ? "walls" : placing === "face" || placing === "line" ? "manual" : null;
    const onDraft = draftKey(e, draftKind, drawing ? drawing.length : manualDraft.length);
    if (onDraft) {
      e.preventDefault();
      if (onDraft === "drop") {
        if (drawing) drawing.pop();
        else manualDraft = manualDraft.slice(0, -1);
      }
      return;
    }
    if (undoKey(e)) {
      e.preventDefault();
      undoKey(e) === "redo" ? redo() : undo();
    } else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (mod && e.key.toLowerCase() === "d" && selectedItem && !drawing) {
      e.preventDefault();
      duplicateItem(selectedItem);
    } else if (!drawing && !placing && dormerSel === null && !openSel && !manualSel && project?.parts.length && wallStepKey(e, false)) {
      // PageDown / PageUp, Alt+→ / Alt+←: the next or previous wall (UX-006).
      e.preventDefault();
      selection = stepWall(project, selection, wallStepKey(e, false));
    } else if (!mod && !drawing && !placing && e.key.startsWith("Arrow") && nudge(e)) {
      // handled
    } else if (prefs.layout !== "3d" && zoomKey(e)) {
      // + / − zoom the plan as the toolbar's buttons do (UX-805).
      e.preventDefault();
      plan?.zoomBy(zoomKey(e)!);
    } else if (e.key === "F3") {
      e.preventDefault();
      setLayout(prefs.layout === "3d" ? "split" : "3d");
    } else if (isDrawKey(e)) {
      // Insert, or Ctrl+B / ⌘B: a Mac keyboard has no Insert key (UX-011).
      e.preventDefault();
      startDraw();
    } else if (e.key === "Enter" && !mod && placing === "face" && manualDraft.length >= 3 && !(e.target as HTMLElement).closest?.("button, a")) {
      // Enter closes a hand-drawn face, as it closes a footprint (UX-010).
      e.preventDefault();
      closeFace();
    } else if (e.key === "Escape" && placing) {
      placing = false;
    } else if (e.key === "Escape" && linePin) {
      linePin = null;
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
  <header inert={closed}>
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
        <strong>{actorName(live.status.actor)}</strong><span class="msg">{pillText(live.status)}</span>
      {:else}
        <Icon name="sparkle" size={14} />{t("ai.short")}
      {/if}
    </button>
    <LanguagePicker />
  </header>

  {#if !loadError}
  <div class="toolbar" class:tight={toolLevel >= 1} class:compact-minor={toolLevel >= 2 && !prefs.toolLabels} class:compact={compactTools && !prefs.toolLabels} class:labels={prefs.toolLabels} role="toolbar" bind:this={toolbarEl} use:hscroll inert={closed || !project} aria-busy={!project}>
    <div class="group">
      <button class="tool" onclick={undo} disabled={at <= 0} title="{t('editor.undo')} ({keys('Ctrl+Z')})"><Icon name="undo" /><span>{t("editor.undo")}</span></button>
      <button class="tool" onclick={redo} disabled={at >= historyLen - 1} title="{t('editor.redo')} ({keys('Ctrl+Shift+Z')})"><Icon name="redo" /><span>{t("editor.redo")}</span></button>
    </div>
    <div class="group">
      <button class="tool" class:on={!!std} aria-pressed={!!std} onclick={() => (std = { part: null, mode: project?.parts.length && selection ? "replace" : "add" })} title={t("tool.standardHint")}><Icon name="house" /><span>{t("tool.standard")}</span></button>
      <button class="tool" class:on={!!drawing} aria-pressed={!!drawing} onclick={toggleDraw} title="{t('tool.drawHint')} ({drawKeyHint(isMac)})"><Icon name="draw" /><span>{t("tool.draw")}</span></button>
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
    <div class="group minor">
      <button class="tool" class:on={prefs.layout === "plan"} aria-pressed={prefs.layout === "plan"} onclick={() => setLayout("plan")} title={t("editor.plan")}><Icon name="plan" /><span>{t("editor.plan")}</span></button>
      <button class="tool" class:on={prefs.layout === "split"} aria-pressed={prefs.layout === "split"} onclick={() => setLayout("split")} title={t("editor.both")}><Icon name="split" /><span>{t("editor.both")}</span></button>
      <button class="tool" class:on={prefs.layout === "3d"} aria-pressed={prefs.layout === "3d"} onclick={() => setLayout("3d")} title="{t('editor.modelHint')} (F3)"><Icon name="cube" /><span>{t("editor.model")}</span></button>
      <button class="tool" onclick={() => plan?.fit()} disabled={prefs.layout === "3d"} title={t("plan.fitHint")}><Icon name="fit" /><span>{t("plan.fit")}</span></button>
      <!-- Zoom without a wheel (touch, pen, keyboard; UX-703). -->
      <button class="tool zoom" onclick={() => plan?.zoomBy(1.25)} disabled={prefs.layout === "3d"} aria-label={t("plan.zoomIn")} title="{t('plan.zoomIn')} (+)"><Icon name="plus" /></button>
      <button class="tool zoom" onclick={() => plan?.zoomBy(0.8)} disabled={prefs.layout === "3d"} aria-label={t("plan.zoomOut")} title="{t('plan.zoomOut')} (−)"><Icon name="minus" /></button>
      <button class="tool" class:on={showDims} aria-pressed={showDims} onclick={() => (showDims = !showDims)} title={t("tool.dims")}><Icon name="dims" /><span>{t("tool.dims")}</span></button>
    </div>
    <div class="group minor">
      <button class="tool" class:on={exporting} aria-pressed={exporting} disabled={!result || !!blockedReason} onclick={() => (exporting = true)} title={blockedReason ?? t("tool.exportHint")}><Icon name="download" /><span>{t("tool.export")}</span></button>
    </div>
  </div>
  {/if}

  {#if project}
    <div class="body">
      <div class="main" use:inertBut={{ on: closed, keep: reviewOut ? "review" : "views" }}>
        {#if pending}
          <!-- Not inert while Revert is out: its pressed button keeps the keyboard's focus; the banner swallows input itself (UI-1103). -->
          <div class="review" class:open={reviewDetails} role="status" tabindex="-1" bind:this={bannerEl} use:refuseInput={{ on: reviewOut, pass: passUndoKeys }}>
            <Icon name="sparkle" size={18} />
            <div class="review-text">
              <strong>
                {t("review.title", { who: actorName(pending.actor) })}<!-- The steps Revert all takes back: a creation is listed, not counted (UI-1802). -->{#if revertSteps > 1}<span class="steps">{t("review.steps", { n: revertSteps })}</span>{/if}
              </strong>
              <!-- Steps none of which stands any more are history: only the AI tab lists them (UX-2001). -->
              {#if review?.steps_stand !== false}<span class="review-summary" title={pendingList.map((a) => entryText(a, activity)).reverse().join(" → ")}>{pendingList.map((a) => entryText(a, activity)).reverse().join(" → ")}</span>{/if}
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
            <!-- Only in small windows (CSS): the banner is one line there. -->
            <button class="btn sm review-more" aria-expanded={reviewDetails} aria-label={t("review.details")} title={t("review.details")} onclick={() => (reviewDetails = !reviewDetails)}><Icon name="list" size={14} /></button>
            {#if review?.before}
              <!-- A check, not a look-alike of the other buttons: "Show previous state" ticked while it shows (UX-008). -->
              <button class="btn sm before" class:on={showBefore} aria-pressed={showBefore} onclick={() => (showBefore = !showBefore)} aria-label={t("review.before")} title={t("review.beforeHint")} data-testid="review-before"><span class="chk" aria-hidden="true">{#if showBefore}<Icon name="check" size={11} />{/if}</span><span class="lbl">{t("review.before")}</span></button>
            {/if}
            {#if onlyCreation}
              <!-- The AI created the job and nothing else waits: there is no version to go back to (HUNT-1601). -->
              <button class="btn sm" aria-disabled={reviewBusy} aria-busy={reviewBusy} onclick={() => !reviewBusy && pressBanner(() => reviewAction(deleteJobNow))} aria-label={t("review.deleteJob")} title={t("review.deleteJobHint")} data-testid="review-delete"><Icon name="trash" size={14} /><span class="lbl">{t("review.deleteJob")}</span></button>
            {:else}
              <button class="btn sm" aria-disabled={reviewBusy} aria-busy={reviewBusy} onclick={() => !reviewBusy && pressBanner(() => revert(true))} aria-label={revertSteps > 1 ? t("review.revertAll") : t("ai.revert")} title={revertSteps > 1 ? t("review.revertAll") : t("ai.revert")}><Icon name="undo" size={14} /><span class="lbl">{revertSteps > 1 ? t("review.revertAll") : t("ai.revert")}</span></button>
            {/if}
            <button class="btn sm accept" aria-disabled={reviewBusy} aria-busy={reviewBusy} title={t("review.accept")} onclick={() => !reviewBusy && pressBanner(accept)}><Icon name="check" size={14} />{t("review.accept")}</button>
          </div>
        {/if}
        {#if calcError}
          {@const d = describeCalcError(calcError, project)}
          <div class="warn-bar" role="alert">
            <i class="dot"></i>
            <div class="warn-text">
              <strong>{d.title}</strong>
              <span>{d.hint}</span>
            </div>
            <code>{d.code}</code>
            {#if calcError.parts?.length && project.parts[calcError.parts[0]]}
              <button class="btn sm" onclick={() => (selection = { part: calcError!.parts![0], edge: 0 })}>{t("opening.show")}</button>
            {/if}
            {#if at > 0}<button class="btn sm" onclick={undo}><Icon name="undo" size={14} />{t("editor.undo")}</button>{/if}
          </div>
        {/if}
        {#if result?.problems?.length}
          {@const first = result.problems[0]}
          <div class="warn-bar" role="alert">
            <i class="dot"></i>
            <div class="warn-text">
              <strong>{t("prob.title", { n: result.problems.length })}</strong>
              <!-- Clamped to two lines in small windows (CSS, UI-205); the whole text stays in the title. -->
              <span title={describeProblem(first, project)}>{describeProblem(first, project)}{#if result.problems.length > 1} · {t("review.more", { n: result.problems.length - 1 })}{/if}</span>
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
          <!-- Under the first view's own label, so it covers neither view's label nor the 3D buttons (UI-309). -->
          {#snippet calcMark()}
            {#if calcPending}<span class="calc-mark" role="status" data-testid="calc-pending"><i class="dot"></i>{t("res.calculating")}</span>{/if}
          {/snippet}
          {#if prefs.layout !== "3d"}
            <div class="view">
              <span class="view-label">{t("editor.plan")}</span>
              {@render calcMark()}
              <PlanView
                bind:this={plan}
                {project}
                {result}
                {highlight}
                {highlightItems}
                {ghost}
                wallNotes={reviewNotes}
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
                oninsert={insertVertex}
                ondrawpoint={addDrawPoint}
                ondrawclose={finishDraw}
                oncursor={(p) => (cursor = p)}
                ondrag={dragObject}
                ondragarm={armDrag}
                ondragend={endDragPress}
                moving={dragState === "moving"}
                locked={dragState === "settling"}
                oncontext={openMenu}
                onitemdbl={openProperties}
                {lineMarks}
                onlinehover={(l) => (lineHover = l ? { ...l, view: "plan" } : null)}
                onlinepick={(l) => pinLine(l, "plan")}
              />
              {#if lineDetail && (lineShown?.view === "plan" || prefs.layout === "plan")}
                <LineInfo detail={lineDetail} {project} pinned={!lineHover && !!linePin} onclose={() => (linePin = null)} />
              {/if}
            </div>
          {/if}
          {#if prefs.layout !== "plan"}
            <div class="view">
              <span class="view-label">{t("editor.model")}</span>
              {#if prefs.layout === "3d"}{@render calcMark()}{/if}
              <Roof3D {project} {result} {selection} item={selectedItem}
                onpick={(tg) => { if (inputClosed()) return; if (tg.type === "item") selectItem(tg.item); else if (tg.type === "edge") { selectItem(null); selection = { part: tg.part, edge: tg.edge }; } }}
                oncontext={openMenu} onitemdbl={openProperties}
                {lineMarks}
                onlinehover={(l) => (lineHover = l ? { ...l, view: "3d" } : null)}
                onlinepick={(l) => { if (!inputClosed()) pinLine(l, "3d"); }} />
              {#if lineDetail && (lineShown?.view === "3d" || prefs.layout === "3d")}
                <LineInfo detail={lineDetail} {project} pinned={!lineHover && !!linePin} onclose={() => (linePin = null)} />
              {/if}
            </div>
          {/if}
        </div>
        {#if drawing}
          <DrawBar
            count={drawing.length}
            prev={lastStep(drawing)}
            onstep={drawStep}
            onundo={() => drawing?.pop()}
            onclose={finishDraw}
            oncancel={(ask) => cancelDraw(!!ask)}
          />
        {:else if placing}
          <PlaceBar kind={placing} points={manualDraft.length}
            onundo={() => (manualDraft = manualDraft.slice(0, -1))}
            onclose={placing === "face" ? closeFace : undefined}
            oncancel={() => (placing = false)} />
        {:else if dormerSel !== null}
          <DormerBar {project} {result} bind:index={dormerSel} onchange={setDormer} ondelete={deleteDormer} onedit={editDormer} edit={barEdit}
            onplace={(e, l, r) => (dormerSel !== null ? placeItem({ kind: "dormer", index: dormerSel }, e, l, r) : undefined)} />
        {:else if openSel}
          <OpeningBar {project} bind:sel={openSel} onchimney={setChimney} onwindow={setWindow} ondelete={deleteOpening} edit={barEdit}
            placement={openSel ? placementOf(result, openSel) : null}
            onplace={(e, l, r) => (openSel ? placeItem({ kind: openSel.kind, index: openSel.index }, e, l, r) : undefined)} />
        {:else if manualSel}
          <ManualBar {project} bind:sel={manualSel} online={setManualLine} onface={setManualFace} ontopart={faceToPart} busy={acting.face} ondelete={deleteManual} />
        {:else}
          <RoofDefBar
            {project}
            {result}
            ondraw={startDraw}
            onstandard={() => (std = { part: null, mode: "add" })}
            bind:selection
            onchange={patchEdge}
            onapplyall={applyAll}
            onheight={setPartHeight}
            onlength={setLength}
            onpoint={(x, y) => { if (selection) void setCorner(selection.part, selection.edge, x, y); }}
            ondelete={deleteVertex}
            onmarker={insertMarker}
            onround={opposite !== null ? openRound : undefined}
            oncalc={() => selection && openCalc(selection.part, selection.edge)}
          />
        {/if}
      </div>

      <aside class:open={dockOpen} inert={dragState !== "idle"} use:refuseInput={{ on: reviewOut, pass: passUndoKeys }}>
        <div class="tabs" role="tablist" use:tablist use:fitlabels>
          <button role="tab" id={dockIds.tab("results")} aria-controls={dockIds.panel} aria-selected={dock === "results"} class:on={dock === "results"} onclick={() => showDock("results")} title={t("dock.results")} aria-label={roofWarned ? t("dock.resultsWarned") : undefined}><Icon name="chart" size={15} /><span class="tab-label">{t("dock.results")}</span>{#if roofWarned}<i class="badge warned" data-testid="results-warned"></i>{/if}</button>
          <button role="tab" id={dockIds.tab("breakdown")} aria-controls={dockIds.panel} aria-selected={dock === "breakdown"} class:on={dock === "breakdown"} onclick={() => showDock("breakdown")} title={t("bd.title")}><Icon name="sigma" size={15} /><span class="tab-label">{t("dock.breakdown")}</span></button>
          <button role="tab" id={dockIds.tab("ai")} aria-controls={dockIds.panel} aria-selected={dock === "ai"} class:on={dock === "ai"} onclick={() => showDock("ai")} title={t("dock.ai")}>
            <Icon name="sparkle" size={15} /><span class="tab-label">{t("dock.ai")}</span>{#if pending}<i class="badge"></i>{/if}
          </button>
          <button role="tab" id={dockIds.tab("job")} aria-controls={dockIds.panel} aria-selected={dock === "job"} class:on={dock === "job"} onclick={() => showDock("job")} title={t("dock.job")}><Icon name="briefcase" size={15} /><span class="tab-label">{t("dock.job")}</span></button>
        </div>
        <div class="dock" role="tabpanel" id={dockIds.panel} aria-labelledby={dockIds.tab(dock)} bind:this={dockEl}>
          <button class="icon-btn dock-close" onclick={() => (dockOpen = false)} aria-label={t("common.close")} title={t("common.close")}><Icon name="x" size={16} /></button>
          {#if dock === "results"}
            <Results {result} {project} recalculated={recalculatedResult} received={receivedResult} onrestore={restoreOffer} restoring={reviewBusy} onstore={storeResult} storing={acting.store} blocked={blockedReason} failure={calcFailure} pending={calcPending} />
          {:else if dock === "breakdown"}
            <Breakdown {project} {result} bind:data={breakdown} onmark={(mk) => (planMark = mk)} />
          {:else if dock === "ai"}
            <AiPanel {id} {project} {activity} recalculation={recalculatedResult} bind:shown={shownEntry} onrevert={(ts) => revert(false, ts)} onrestore={restoreOffer} busy={reviewBusy} {onconnect} onfocus={(c) => (focusChange = c)} />
          {:else}
            <JobPanel {id} {project} current={() => (destroyed ? null : project)} onchange={commit} onsave={save} blocked={blockedReason} recalculated={!!recalculatedResult} edits={jobEdits} track={(p) => pendingEdits.track(p)} onresulttoast={resultToast} />
          {/if}
        </div>
      </aside>
    </div>

    <footer class="statusbar" inert={closed}>
      <span class="coords">{cursor ? `X ${num(cursor.x)}   Y ${num(cursor.y)}` : "X —   Y —"}</span>
      <span class="opt">{t("status.snap")}</span>
      <span class="opt">{t("status.walls", { n: wallCount, p: project.parts.length })}</span>
      {#if result}<span data-testid="status-area" class:pending={calcPending} aria-busy={calcPending}>{t("res.area")}: <b title={roofState(result) !== "ok" ? t(roofState(result) === "none" ? "res.notCalculated" : "res.calcPartial") : undefined}>{roofState(result) === "none" ? "—" : m2(result.totals.roof_area)}</b></span>{/if}
      <span class="spacer"></span>
      <span class="save" class:failed={!reviewOut && saveState === "failed"}>
        {#if reviewOut}{t(reviewStatusKey(reviewWhat))}{:else if saveState === "saved" && !editsWaiting && outsideUnreadable}<span class="state-text" title={t("editor.outsideState")}>{t("editor.outsideState")}</span>{:else if saveState === "saved" && !editsWaiting && outsideNewer}<span class="state-text" title={t("editor.outsideNewerState")} data-testid="status-newer">{t("editor.outsideNewerState")}</span>{:else if saveState === "saved" && !editsWaiting}<Icon name="check" size={13} />{t("editor.saved")}{:else if saveState === "saving" || saveState === "saved"}{t("editor.saving")}{:else if saveState === "failed"}<span class="save-failed"><i class="dot"></i>{t("editor.saveFailed")}</span><button class="retry" onclick={save}>{t("err.retry")}</button>{:else}{t("editor.dirty")}{/if}
      </span>
    </footer>
  {:else if !loadError}
    <!-- The job is being read (its stored result may be recalculated first): said so, and the tools
         above wait for it, instead of an empty page with live tools (audit 2026-11-22 ERR-803). -->
    <div class="loading" role="status" data-testid="editor-loading"><span>{t("editor.loading")}</span></div>
  {:else}
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

{#if objDlg && openSel && project}
  <Modal title={itemTitle(openSel)} width={760} onclose={() => closeObjDlg(false)}>
    <div class="obj-dlg" data-testid="object-dialog">
      <OpeningBar {project} bind:sel={openSel} onchimney={setChimney} onwindow={setWindow} ondelete={() => { objDlg = false; deleteOpening(); }} edit={barEdit}
        placement={placementOf(result, openSel)}
        onplace={(e, l, r) => (openSel ? placeItem({ kind: openSel.kind, index: openSel.index }, e, l, r) : undefined)} />
    </div>
    {#snippet footer()}
      <button class="btn" onclick={() => closeObjDlg(false)}>{t("common.cancel")}</button>
      <button class="btn primary" data-testid="object-dialog-ok" onclick={() => closeObjDlg(true)}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

{#if dormerDlg && project}
  <Modal title={dormerDlg.index === null ? t("dormer.dialogNew") : t("dormer.n", { n: dormerDlg.index + 1 })} width={1040} onclose={() => (dormerDlg = null)}>
    <DormerDialog bind:this={dormerDialog} {project} roofFailed={!!roofError} fresh={dormerDlg.index === null} bind:params={dormerDlg.params} bind:blocked={dormerBlocked} bind:notice={dormerNotice} track={(p) => pendingEdits.track(p)} />
    {#snippet footer()}
      {#if dormerBlocked}<p class="dlg-why" role="alert" data-testid="dormer-ok-blocked">{dormerBlocked}</p>
      {:else if dormerNotice}<p class="dlg-why" role="status" data-testid="dormer-ok-notice">{dormerNotice}</p>{/if}
      <button class="btn" onclick={() => (dormerDlg = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={!!dormerBlocked} aria-disabled={acting.dormer} aria-busy={acting.dormer} title={dormerBlocked ?? undefined} onclick={() => !acting.dormer && applyDormerDialog()}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

{#if calc && project?.parts[calc.part]}
  <Modal title={t("calc.title")} width={760} onclose={() => (calc = null)}>
    <RoofCalcForm {project} partIndex={calc.part} edge={calc.edge} bind:request={calc.request} />
    {#snippet footer()}
      <button class="btn" onclick={() => (calc = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={!calc?.request} aria-disabled={acting.calc} aria-busy={acting.calc} onclick={() => !acting.calc && applyCalc()}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

{#if round}
  <Modal title={t("round.title")} width={720} onclose={() => (round = null)}>
    <RoundRoofForm span={round.span} bind:eave={round.eave} bind:overhang={round.overhang} bind:arc={round.arc} bind:request={round.request} />
    {#snippet footer()}
      <button class="btn" onclick={() => (round = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={!round?.arc} aria-disabled={acting.round} aria-busy={acting.round} onclick={() => !acting.round && applyRound()}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

{#if xform && project?.parts[xform.part]}
  <Modal title={t("xf.title")} width={680} onclose={() => (xform = null)}>
    <TransformForm part={project.parts[xform.part]} edge={xform.edge} bind:mode={xform.mode} bind:op={xform.op} />
    {#snippet footer()}
      <button class="btn" onclick={() => (xform = null)}>{t("common.cancel")}</button>
      <button class="btn primary" disabled={!xform?.op} aria-disabled={acting.transform} aria-busy={acting.transform} onclick={() => !acting.transform && applyTransform()}>{t("common.ok")}</button>
    {/snippet}
  </Modal>
{/if}

{#if std}
  <Modal title={t("std.title")} width={780} onclose={() => (std = null)}>
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
  /* Why the dormer dialog's OK is off: a small status mark and the reason, left of the buttons. */
  .dlg-why {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 auto 0 0;
    max-width: 60%;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .dlg-why::before {
    content: "";
    flex: none;
    width: 8px;
    height: 8px;
    background: var(--danger);
  }
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
  .tool.zoom {
    min-width: 36px;
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
    flex-shrink: 0;
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
  /* The job is being read: one quiet line, shown only when the read takes a moment. */
  .loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-2);
    font-size: 13px;
  }
  .loading span {
    animation: loading-in 0s linear 0.3s both;
  }
  @keyframes loading-in {
    from {
      visibility: hidden;
    }
    to {
      visibility: visible;
    }
  }
  .toolbar[aria-busy="true"] {
    opacity: 0.5;
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
  .btn.before .chk {
    display: inline-grid;
    place-items: center;
    width: 14px;
    height: 14px;
    border: 1.5px solid var(--text-3);
    border-radius: 3px;
  }
  .btn.before.on .chk {
    border-color: var(--ai-strong);
    background: var(--ai-strong);
    color: var(--surface);
  }
  .btn.accept {
    background: var(--ai);
    border-color: var(--ai);
    color: var(--on-ai);
  }
  .views {
    position: relative;
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
  /* A recalculation on its way: the plan and 3D still show the roof before the edit (ERR-204). */
  .calc-mark {
    position: absolute;
    z-index: 3;
    left: 10px;
    top: 34px;
    max-width: calc(100% - 20px);
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    font-size: 11.5px;
    color: var(--text-2);
    pointer-events: none;
  }
  .calc-mark .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .statusbar .pending b {
    color: var(--text-3);
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
  /* Labels that would be cut ("KI" to "K") are hidden; icons and tooltips stay (UI-005, fitlabels.ts). */
  .tabs:global(.compact) .tab-label {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
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
  /* A warning in the result: the square of the warning mark (Toasts, Results), not the AI's dot. */
  .badge.warned {
    border-radius: 1px;
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
    /* A long state text shrinks with an ellipsis; it never widens the bar
       (UI-1602). The box itself does not clip: Retry stays whole (UI-1701). */
    min-width: 0;
  }
  /* A failed save: the state and Retry keep their room, the hints beside them give way (UI-1701). */
  .save.failed {
    flex-shrink: 0;
  }
  .statusbar .opt {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .save .state-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
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
  /* The object bar inside its dialog: no dock border, fields wrap. */
  .obj-dlg :global(.bar) {
    border-top: 0;
    padding: 0;
    min-height: 0;
  }
  .main > :global(.bar) {
    flex: none;
    max-height: 46vh;
    overflow-y: auto;
  }

  /* Labels that do not fit the window (long languages), step by step (UX-006):
     smaller labels on up to two lines, then the view and export buttons icon
     only, then every button icon only, as below. */
  @media (min-width: 1221px) and (min-height: 641px) {
    .toolbar.tight .tool {
      min-width: 44px;
      max-width: 80px;
      padding: 0 4px;
    }
    .toolbar.tight .tool span {
      font-size: 10.5px;
      line-height: 1.1;
      white-space: normal;
      text-align: center;
    }
  }
  .toolbar.compact-minor .group.minor .tool {
    width: 36px;
    height: 36px;
    min-width: 0;
    padding: 0;
  }
  .toolbar.compact-minor .group.minor .tool span {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
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

  /* Settings > "Always show button labels": labels stay in any window,
     smaller and on two lines if needed; the toolbar scrolls sideways when
     even that does not fit. More specific than the rules that hide them. */
  @media (max-width: 1440px), (max-height: 640px) {
    .toolbar.labels .tool {
      width: auto;
      min-width: 44px;
      max-width: 76px;
      height: 50px;
      padding: 0 4px;
    }
    .toolbar.labels .tool span {
      position: static;
      width: auto;
      height: auto;
      overflow: visible;
      clip-path: none;
      font-size: 10.5px;
      line-height: 1.1;
      white-space: normal;
      text-align: center;
    }
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
  /* Small windows: a problem banner keeps the plan usable, its title on one
     line and the text cut after two lines (UI-205). */
  @media (max-height: 640px), (max-width: 760px) {
    .warn-text strong {
      max-width: 100%;
      overflow: hidden;
      white-space: nowrap;
      text-overflow: ellipsis;
    }
    .warn-text span {
      flex-basis: 100%;
      min-width: 0;
      display: -webkit-box;
      -webkit-box-orient: vertical;
      -webkit-line-clamp: 2;
      line-clamp: 2;
      overflow: hidden;
    }
  }
  .review-more {
    display: none;
  }
  /* Small windows: the review banner keeps to one line of text with the
     buttons as icons, and "Details" opens the change list (UX-017). */
  @media (max-height: 640px), (max-width: 760px) {
    .review {
      gap: 8px;
    }
    .review > :global(svg:first-child) {
      display: none;
    }
    .review-more {
      display: inline-flex;
    }
    .review-more[aria-expanded="true"] {
      border-color: color-mix(in srgb, var(--ai) 60%, var(--border));
      color: var(--ai-strong);
    }
    .review .btn.sm {
      padding: 0 9px;
    }
    .review .lbl {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip: rect(0 0 0 0);
      white-space: nowrap;
    }
    .review:not(.open) .review-text strong,
    .review:not(.open) .review-summary {
      overflow: hidden;
      white-space: nowrap;
      text-overflow: ellipsis;
      display: block;
    }
    .review:not(.open) .review-list,
    .review:not(.open) .review-figures {
      display: none;
    }
  }
</style>
