<script lang="ts">
  import { serializePlan } from "./bugreport";
  import { onDestroy, untrack } from "svelte";
  import type { CalcResult, LineRef, PlanLabel, PlanMark, Point2, Project, Selection } from "./types";
  import { materialToken, tiledColour } from "./materials";
  import { isGable } from "./types";
  import { onWall } from "./corners";
  import { t } from "./i18n.svelte";
  import { partLabel } from "./partname";
  import { windowOutline, type OpeningSel } from "./openings";
  import { kindOf, lineLength, type ManualSel } from "./manual";
  import type { Ghost } from "./changes";
  import { placementOf, type ItemRef, type PlanTarget } from "./items";
  import { gridStep as gridStepFor, gridValues } from "./grid";
  import { boxOf, contentPoints, draftLeavesView, fitView, resizedView } from "./planfit";
  import { hostIndex, isDormerFace } from "./dormers";
  import { dormerWallLines, onWallLine } from "./scene3d";
  import { chooseLabels, insidePolygon, placeDimSpots, rotatedBox, segCrossesBox, segIndex, wallNumber, type Box, type DimSpot, type Seg } from "./planlabels";
  import { draftWalls } from "./drawstep";
  import { coveredPieces, visibleFaceTags } from "./planjoin";
  import { numberLocale } from "./i18n.svelte";

  let {
    project,
    result,
    selection = $bindable(),
    onmove,
    oninsert,
    highlight = new Set<string>(),
    highlightItems = new Set<string>(),
    ghost = null,
    wallNotes = new Map<string, string>(),
    mark = null,
    faceLabels = [],
    drawing = null,
    ondrawpoint,
    ondrawclose,
    oncursor,
    showDims = true,
    dormerSel = $bindable(null),
    placing = false,
    onplace,
    openSel = $bindable(null),
    manualSel = $bindable(null),
    manualDraft = [],
    ondrag,
    ondragarm,
    ondragend,
    moving = false,
    locked = false,
    oncontext,
    onitemdbl,
    lineMarks = [],
    onlinehover,
    onlinepick,
  }: {
    project: Project;
    result: CalcResult | null;
    selection: Selection;
    /** A step of a corner drag (armed by `ondragarm`, ended by `ondragend`). */
    onmove: (part: number, vertex: number, x: number, y: number) => void;
    oninsert: (part: number, edge: number, x: number, y: number) => void;
    /** "part:edge" keys of edges changed by the latest AI edit. */
    highlight?: Set<string>;
    /** "field:item" keys (dormer:0, chimney:1, …) of items changed by the AI. */
    highlightItems?: Set<string>;
    /** The state before the pending AI changes, drawn dashed. */
    ghost?: Ghost | null;
    /** "part:edge" -> what the AI changed on that wall ("40° → 35°"), written beside it (UX-017). */
    wallNotes?: Map<string, string>;
    /** A face or line the calculation breakdown points at. */
    mark?: PlanMark;
    /** Face numbers of the calculation breakdown. */
    faceLabels?: PlanLabel[];
    /** Points of a footprint being drawn (draw mode), or null. */
    drawing?: Point2[] | null;
    ondrawpoint?: (x: number, y: number) => void;
    ondrawclose?: () => void;
    oncursor?: (p: Point2 | null) => void;
    showDims?: boolean;
    /** Index of the selected dormer, or null. */
    dormerSel?: number | null;
    /** What the next click places: a dormer's front corner, a chimney, a window or a roof opening. */
    placing?: false | "dormer" | "chimney" | "window" | "hole" | "line" | "face";
    onplace?: (x: number, y: number) => void;
    /** The selected chimney, roof window or roof opening, or null. */
    openSel?: OpeningSel;
    /** The selected hand-drawn line or face, or null. */
    manualSel?: ManualSel;
    /** Points picked so far for a line or face being added. */
    manualDraft?: [number, number, number][];
    /** A step of the drag of an item or the selected part, by (dx, dy) from the press. */
    ondrag?: (target: PlanTarget, dx: number, dy: number) => void;
    /**
     * A press that may become a drag (an item, the selected part, a corner),
     * with its pointer. False: the editor opens no drag, the press does nothing more.
     */
    ondragarm?: (target: PlanTarget, pointerId: number) => boolean;
    /**
     * The end of every press that was armed: release, cancelled or lost
     * pointer, the plan going away; `moved` when it went past the slop.
     */
    ondragend?: (target: PlanTarget, moved: boolean) => void;
    /** A drag is under way: the hint says how to cancel it (UX-604). */
    moving?: boolean;
    /** The editor is closed for input (the last drop is with the core): presses do nothing. */
    locked?: boolean;
    /** Right click (or Ctrl+click) without dragging: open the context menu. */
    oncontext?: (target: PlanTarget, client: { x: number; y: number }, world: Point2) => void;
    /** A double-click on a dormer, chimney, window, opening or hand-drawn shape: open its properties. */
    onitemdbl?: (item: ItemRef) => void;
    /** Roof lines drawn highlighted (the one under the pointer, the pinned one). */
    lineMarks?: LineRef[];
    /** The pointer came onto a roof line (or left it: null). */
    onlinehover?: (line: LineRef | null) => void;
    /** A left click on a roof line: pin its details. */
    onlinepick?: (line: LineRef) => void;
  } = $props();

  let cursorWorld = $state<Point2 | null>(null);

  let svg: SVGSVGElement | undefined = $state();
  let width = $state(800);
  let height = $state(600);
  // View transform: screen = (world - center) * scale + size / 2, y flipped.
  let scale = $state(30);
  let center = $state({ x: 0, y: 0 });
  let fitted = false;
  /** The user zoomed or panned: stop re-fitting the plan when the view is resized. */
  let userView = false;

  const GRID = 0.5;
  const SNAP = 0.05;

  /** Everything the plan should show: the walls, hand-drawn faces and lines, the
   *  eaves of the calculated roof (its labels lie outside them) and a footprint
   *  being drawn (planfit.ts `contentPoints`). */
  function contentBox() {
    const faces = (result?.parts ?? []).flatMap((rp) => rp.geometry.faces.map((f) => f.polygon));
    return boxOf(contentPoints(project, faces, drawing));
  }

  /** Zooms by `factor` about the middle of the plan (the + / − buttons, UX-703). */
  export function zoomBy(factor: number) {
    if (!(factor > 0) || !Number.isFinite(factor)) return;
    scale = Math.min(400, Math.max(4, scale * factor));
    userView = true;
  }

  /** The plan as the user sees it, as a standalone SVG file (bug report); null when it cannot be taken. */
  export function snapshotSvg(): string | null {
    return svg ? serializePlan(svg) : null;
  }

  export function fit() {
    const box = contentBox();
    if (!box) return;
    const v = fitView(box, width, height);
    if (v === null) return;
    userView = false;
    scale = v.scale;
    center = v.center;
  }

  $effect(() => {
    if (!fitted && width > 10) {
      // A job of hand-drawn faces only is fitted too (UX-301).
      if (contentBox()) fit();
      else {
        scale = 30;
        center = { x: 8, y: 5 };
      }
      fitted = true;
      size = { width, height };
    }
  });

  /** The plan size the view was last laid out for. */
  let size = { width: 0, height: 0 };
  // Window resized (e.g. snapped to half the screen) or a bar below the plan grew:
  // keep the whole roof in view until the user zooms or pans. A bar that only
  // changes the height keeps the drawing still on screen (UI-011).
  $effect(() => {
    const to = { width, height };
    untrack(() => {
      const from = size;
      size = to;
      if (!fitted || userView || !contentBox() || (from.width === to.width && from.height === to.height)) return;
      const next = resizedView({ scale, center }, from, to, contentBox());
      if (next === "fit") fit();
      else center = next.center;
    });
  });

  // A footprint being drawn (lengths typed in the draw bar) stays in view: when
  // its newest corner leaves the plan, the plan fits the drawing (UX-001).
  // When a removed corner was what zoomed the plan out (a mistyped 99 m wall),
  // the plan fits the drawing that is left (UX-203).
  let draftLength = 0;
  let draftFollowed = false;
  $effect(() => {
    const d = drawing;
    const n = d?.length ?? 0;
    untrack(() => {
      const before = draftLength;
      draftLength = n;
      if (n === 0) draftFollowed = false;
      if (!d || n < 2 || !fitted) return;
      if (n < before && draftFollowed && !userView) {
        fit();
        return;
      }
      if (draftLeavesView(d, { scale, center }, width, height)) {
        fit();
        draftFollowed = true;
      }
    });
  });

  const snapPoint = (px: number, py: number, alt: boolean) => {
    const snap = alt ? 0.001 : SNAP;
    return { x: +(Math.round(wx(px) / snap) * snap).toFixed(3), y: +(Math.round(wy(py) / snap) * snap).toFixed(3) };
  };

  // Pitch and area of every calculated face, written at its centre.
  // Joined roofs: stretches of a line under a higher roof are drawn dashed.
  // The highest roof over a plan point, built once per result from a plain
  // copy (a job of 200 buildings froze the plan when every sample read every
  // face through the reactive result, UI-501).
  const topAt = $derived(hostIndex(result));
  const linePieces = $derived(result ? coveredPieces(result, topAt) : []);
  /** Faces of a material other than tiles, tinted in its colour (sheet metal grey, …). */
  const materialFaces = $derived(
    (result?.parts ?? []).flatMap((rp) => rp.geometry.faces.filter((f) => f.polygon.length >= 3 && !tiledColour(f.material))),
  );
  /** The highlighted roof lines' end points. */
  const markedLines = $derived(
    lineMarks.map((m) => result?.parts[m.part]?.geometry.lines[m.index]).filter((l) => !!l),
  );
  const hovering = () => !drag && !objDrag && !pan && !placing && !drawing;
  // A face under another building's higher roof gets no label: it would be
  // written over the roof that covers it (UX-012).
  const tags = $derived(result ? visibleFaceTags(result, topAt) : []);
  // One formatter per language, not one per label: a plan of 200 buildings
  // made thousands of them at every zoom step (UX-602).
  const pitchFmt = $derived(new Intl.NumberFormat(numberLocale(), { maximumFractionDigits: 2 }));
  const areaFmt = $derived(new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }));
  const lenFmt = $derived(new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 2, useGrouping: false }));
  const pitchText = (v: number) => `${pitchFmt.format(v)}°`;
  const areaText = (v: number) => `${areaFmt.format(v)} m²`;
  /** A length in the plan, 2 decimals in the UI language (decimal comma in cs, sk, de). */
  const lenText = (v: number) => lenFmt.format(v);
  /**
   * The outlines of the buildings as plain numbers with their plan box and
   * turning sense, once per change of the job: the label pass below reads
   * them at every zoom and pan step (UX-602).
   */
  const outlinesOf = $derived(
    project.parts.map((part) => {
      const pts = part.edges.map((e) => ({ x: e.start.x, y: e.start.y }));
      // Shoelace sign: > 0 means counter-clockwise (y up), outside is right of travel.
      let area2 = 0;
      let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
      pts.forEach((p, k) => {
        const q = pts[(k + 1) % pts.length];
        area2 += p.x * q.y - q.x * p.y;
        if (p.x < x0) x0 = p.x;
        if (p.x > x1) x1 = p.x;
        if (p.y < y0) y0 = p.y;
        if (p.y > y1) y1 = p.y;
      });
      return { pts, ccw: area2 >= 0, x0, x1, y0, y1 };
    }),
  );
  /** The roof lines inside the roofs (not eaves and verges) in plan metres, once per result. */
  const innerLines = $derived(
    linePieces.flatMap((ls) => ls.filter((l) => l.kind !== "eave" && l.kind !== "verge").flatMap((l) => l.pieces.map((pc): Seg => [pc.a[0], pc.a[1], pc.b[0], pc.b[1]]))),
  );
  /** A placement distance, 3 decimals like DACAD's dialogs (mm). */
  const mmText = (v: number) =>
    new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 3, maximumFractionDigits: 3, useGrouping: false }).format(v);
  /** The selected dormer, chimney, window or opening, and where it sits on its face (dimension lines). */
  const placed = $derived.by(() => {
    const it: ItemRef | null = dormerSel !== null ? { kind: "dormer", index: dormerSel } : openSel ? { kind: openSel.kind, index: openSel.index } : null;
    return showDims ? placementOf(result, it) : null;
  });
  /** The calculated roof faces of each dormer, so a press anywhere on the
      dormer picks it, not only on its front line (review UX-003). */
  const dormerFaces = $derived.by(() => {
    const out: Point2[][][] = project.dormers.map(() => []);
    if (!result) return out;
    const lines = project.dormers.map(dormerWallLines);
    for (const rp of result.parts) {
      for (const f of rp.geometry.faces) {
        if (!isDormerFace(f.edge) || f.polygon.length < 3) continue;
        const d = lines.findIndex((ls) => f.polygon.some((p, k) => onWallLine(ls, p, f.polygon[(k + 1) % f.polygon.length])));
        if (d >= 0) out[d].push(f.polygon.map((p) => ({ x: p[0], y: p[1] })));
      }
    }
    return out;
  });
  /** One dimension line in screen space with end ticks; null when too short to draw. */
  function dimLine(a: Point2, b: Point2) {
    const [x1, y1, x2, y2] = [sx(a.x), sy(a.y), sx(b.x), sy(b.y)];
    const len = Math.hypot(x2 - x1, y2 - y1);
    if (len < 1) return { x1, y1, x2, y2, ticks: "", mx: x1, my: y1, short: true };
    const [nx, ny] = [(-(y2 - y1) / len) * 4, ((x2 - x1) / len) * 4];
    const ticks = `M${x1 - nx},${y1 - ny}L${x1 + nx},${y1 + ny}M${x2 - nx},${y2 - ny}L${x2 + nx},${y2 + ny}`;
    return { x1, y1, x2, y2, ticks, mx: (x1 + x2) / 2, my: (y1 + y2) / 2, short: len < 18 };
  }

  // Label heights in px with their text halo: an 11 px wall length, a face
  // label with pitch (16 px) and area (12.5 px), the pitch alone.
  const DIM_H = 16;
  const TAG_FULL_H = 40;
  const TAG_PITCH_H = 23;

  /** Rough width of a label in px at the given font size. */
  const textPx = (s: string, size: number) => s.length * size * 0.62 + 8;

  const sx = (x: number) => (x - center.x) * scale + width / 2;
  const sy = (y: number) => -(y - center.y) * scale + height / 2;
  const wx = (px: number) => (px - width / 2) / scale + center.x;
  const wy = (py: number) => -(py - height / 2) / scale + center.y;

  /** Spacing of the grid lines drawn at this zoom, m (never denser than 8 px). */
  const gridStep = () => gridStepFor(scale, GRID);

  // Magnet radius, px [no source: a few pixels, as drawing programs use];
  // at most 40 % of a grid cell, so 5 cm steps stay reachable between lines.
  const MAGNET_PX = 12;
  /** Grid or corner lines the pointer is held on (shown as guides). */
  let guides = $state<{ x: number | null; y: number | null }>({ x: null, y: null });

  /**
   * The point for a corner, a new wall point or a placed object: pulled onto
   * the visible grid lines and onto the x / y of other corners when within a
   * few pixels of them, else rounded to 5 cm. Alt: 1 mm, no magnet.
   * `skip` is the corner being dragged. O(corners).
   */
  function magnetPoint(px: number, py: number, alt: boolean, skip?: { part: number; vertex: number }): Point2 {
    if (alt) {
      guides = { x: null, y: null };
      return snapPoint(px, py, true);
    }
    const step = gridStep();
    const r = Math.min(MAGNET_PX, step * scale * 0.4);
    let bx = { v: null as number | null, d: r };
    let by = { v: null as number | null, d: r };
    const gx = Math.round(wx(px) / step) * step;
    const gy = Math.round(wy(py) / step) * step;
    if (Math.abs(sx(gx) - px) <= bx.d) bx = { v: gx, d: Math.abs(sx(gx) - px) };
    if (Math.abs(sy(gy) - py) <= by.d) by = { v: gy, d: Math.abs(sy(gy) - py) };
    const skipAt = skip ? project.parts[skip.part]?.edges[skip.vertex]?.start : undefined;
    const consider = (q: Point2) => {
      const dx = Math.abs(sx(q.x) - px);
      const dy = Math.abs(sy(q.y) - py);
      // A corner's own line wins over the grid at the same distance.
      if (dx <= bx.d) bx = { v: q.x, d: dx };
      if (dy <= by.d) by = { v: q.y, d: dy };
    };
    project.parts.forEach((part, pi) =>
      part.edges.forEach((e) => {
        if (skipAt && pi === skip!.part && e.start.x === skipAt.x && e.start.y === skipAt.y) return;
        consider(e.start);
      }),
    );
    for (const q of drawing ?? []) consider(q);
    guides = { x: bx.v, y: by.v };
    const fine = snapPoint(px, py, false);
    return { x: bx.v !== null ? +bx.v.toFixed(3) : fine.x, y: by.v !== null ? +by.v.toFixed(3) : fine.y };
  }

  // Drawing or placing finished: drop its guides.
  $effect(() => {
    if (!drawing && !placing) untrack(() => (guides.x !== null || guides.y !== null) && !drag && (guides = { x: null, y: null }));
  });

  const gridLines = $derived.by(() => {
    const step = gridStep();
    const out: { d: string; major: boolean }[] = [];
    for (const x of gridValues(wx(0), wx(width), step)) {
      out.push({ d: `M${sx(x)},0V${height}`, major: Math.abs(x / (step * 5) - Math.round(x / (step * 5))) < 1e-6 });
    }
    for (const y of gridValues(wy(height), wy(0), step)) {
      out.push({ d: `M0,${sy(y)}H${width}`, major: Math.abs(y / (step * 5) - Math.round(y / (step * 5))) < 1e-6 });
    }
    return out;
  });

  // Dragging. One press at a time, and every `onpointerdown` of the plan
  // goes through `press` (planpress.test.ts): a handler of its own forgot
  // the other-pointer check in round after round (ERR-501, UI-601).
  let drag: { part: number; vertex: number; sx: number; sy: number; moved: boolean } | null = null;
  let pan: { x: number; y: number; cx: number; cy: number } | null = null;
  /** An item or a whole part being dragged: where the press started. */
  let objDrag: { target: PlanTarget; sx: number; sy: number; wx: number; wy: number; moved: boolean; dx: number; dy: number } | null = null;
  /** The press the editor armed a drag for (`ondragarm`): its end is always told (`ondragend`). */
  let armed: PlanTarget | null = null;
  // A plan taken away mid-drag (another view) ends the drag where it is:
  // the editor holds core-backed edits back until a drag ends (ERR-401).
  // A corner drag ends the same way (UI-502).
  onDestroy(() => endGesture());
  /**
   * The pointer of the press under way (a drag, a pan, a right press). Any
   * other pointer (a second finger, the pen beside the finger) is ignored
   * until it lets go: a second press used to replace the running drag, whose
   * release then never came and the editor's drag pause stayed on (ERR-501).
   */
  let gesture: number | null = null;
  /**
   * `e` is a pointer other than the one of the press under way. A press
   * whose pointer the plan no longer holds (its release went elsewhere, or
   * a synthetic press that could not be captured) is over: it ends here,
   * so it cannot block the plan.
   */
  function otherPointer(e: PointerEvent): boolean {
    if (gesture === null || e.pointerId === gesture) return false;
    let held = false;
    try {
      held = !!svg?.hasPointerCapture(gesture);
    } catch {
      held = false;
    }
    if (held) return true;
    endGesture();
    return false;
  }

  /**
   * Ends the press under way: its release, a cancelled or lost pointer, a
   * mouse that reports no button any more, the plan going away. A moved
   * drag is dropped where it is. The press is cleared first, so a handler
   * that throws cannot leave the plan dragging (errors To check 40), and
   * the editor always hears the end of a press it armed, moved or not.
   */
  function endGesture(e?: PointerEvent) {
    noLongPress();
    const [r, a, moved] = [rc, armed, !!(objDrag?.moved || drag?.moved)];
    gesture = null;
    drag = null;
    objDrag = null;
    pan = null;
    rc = null;
    armed = null;
    if (!drawing && !placing) guides = { x: null, y: null };
    try {
      if (r && e) {
        const p = localPoint(e);
        if (Math.hypot(p.x - r.x, p.y - r.y) <= DRAG_SLOP) oncontext?.(r.target, { x: r.cx, y: r.cy }, snapPoint(r.x, r.y, false));
      }
    } finally {
      if (a) ondragend?.(a, moved);
    }
  }
  /** What a right press hit, set by the shape's own handler before the plan's. */
  let rcTarget: PlanTarget | null = null;
  /** A right press that opens the menu on release unless it pans. */
  let rc: { target: PlanTarget; x: number; y: number; cx: number; cy: number } | null = null;
  // Pixels the pointer may travel before a press counts as a drag.
  const DRAG_SLOP = 4;

  /** Ctrl+click is a right click (macOS habit, and one-button mice). */
  const button = (e: PointerEvent) => (e.button === 0 && e.ctrlKey ? 2 : e.button);

  function selectItem(item: ItemRef) {
    selection = null;
    dormerSel = item.kind === "dormer" ? item.index : null;
    openSel = item.kind === "chimney" || item.kind === "window" || item.kind === "hole" ? { kind: item.kind, index: item.index } : null;
    manualSel = item.kind === "line" || item.kind === "face" ? { kind: item.kind, index: item.index } : null;
  }

  /** Captures the pointer for a press that goes on (a drag, a pan); false
   * when it cannot be captured: a synthetic or already released pointer (no
   * active pointer with that id). A pan or right press then just ends at
   * the next pointerup the plan sees. Only `press` and what it starts call
   * this. */
  function capture(e: PointerEvent): boolean {
    gesture = e.pointerId;
    try {
      svg!.setPointerCapture(e.pointerId);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * The editor opens a drag for `target` (jobedits.ts rule 7) and the plan
   * holds its pointer: false when a drag is live already (it is never
   * replaced), when the editor opens none (a drop still settles, nothing to
   * hold), or when the pointer cannot be captured (the plan could not
   * promise to see its release, and an open drag closes the editor). The
   * press then starts no drag.
   */
  function arm(e: PointerEvent, target: PlanTarget): boolean {
    if (objDrag || drag || armed) return false;
    if (!capture(e)) {
      gesture = null;
      return false;
    }
    if (ondragarm && !ondragarm(target, e.pointerId)) {
      gesture = null;
      try {
        svg!.releasePointerCapture(e.pointerId);
      } catch {
        // Already gone.
      }
      return false;
    }
    armed = target;
    return true;
  }

  function startObjDrag(e: PointerEvent, target: PlanTarget) {
    if (!arm(e, target)) return;
    const p = localPoint(e);
    const w = snapPoint(p.x, p.y, e.altKey);
    objDrag = { target, sx: p.x, sy: p.y, wx: w.x, wy: w.y, moved: false, dx: 0, dy: 0 };
  }

  /** What a press landed on: the plan itself, or one of its shapes. */
  type PressOn =
    | { on: "plan" }
    /** `line`: the press landed on that roof line of the part (its details are pinned, UI-701). */
    | { on: "part"; part: number; line?: number }
    | { on: "edge"; part: number; edge: number }
    | { on: "vertex"; part: number; vertex: number }
    | { on: "item"; item: ItemRef };

  /**
   * Every press in the plan. Its first check is the other pointer: a second
   * finger, the pen or a mouse button beside the press under way does
   * nothing, wherever it lands. While the editor is closed (the last drop
   * is with the core) a new press does nothing either.
   */
  /** The last press on the plan was a finger or a pen: the hint names long press first (UX-805). */
  let touchHint = $state(false);

  function press(e: PointerEvent, at: PressOn) {
    if (otherPointer(e) || locked) {
      if (at.on !== "plan") e.stopPropagation();
      return;
    }
    touchHint = e.pointerType === "touch" || e.pointerType === "pen";
    longPress(e, at);
    if (at.on === "plan") pressPlan(e);
    else if (at.on === "item") pressItem(e, at.item);
    else if (at.on === "part") {
      // A press on a roof line pins its details and is otherwise a press on
      // the building under it (UI-701): the first selects it, the next drags it.
      if (at.line !== undefined && !placing && !drawing && button(e) === 0) onlinepick?.({ part: at.part, index: at.line });
      pressPart(e, at.part);
    }
    else if (at.on === "vertex") pressVertex(e, at.part, at.vertex);
    else pressEdge(e, at.part, at.edge);
  }

  /**
   * A finger or pen held still on the plan opens its menu, as the right
   * button does (UX-703): touch and pen have no right button. A press that
   * moves past LONG_SLOP, starts dragging or ends first opens nothing.
   */
  let longHeld: { id: number; t: number; x: number; y: number; timer: ReturnType<typeof setTimeout> } | null = null;
  const LONG_MS = 550;
  const LONG_SLOP = 8;
  function longPress(e: PointerEvent, at: PressOn) {
    // A shape's press that bubbles on to the plan's is the same press.
    if (longHeld && longHeld.id === e.pointerId && longHeld.t === e.timeStamp) return;
    noLongPress();
    if ((e.pointerType !== "touch" && e.pointerType !== "pen") || e.button !== 0 || placing || drawing) return;
    const target: PlanTarget =
      at.on === "plan" ? { type: "canvas" }
      : at.on === "item" ? { type: "item", item: at.item }
      : at.on === "part" ? { type: "part", part: at.part }
      : at.on === "edge" ? { type: "edge", part: at.part, edge: at.edge }
      : { type: "vertex", part: at.part, vertex: at.vertex };
    const client = { x: e.clientX, y: e.clientY };
    const local = localPoint(e);
    longHeld = {
      id: e.pointerId,
      t: e.timeStamp,
      ...client,
      timer: setTimeout(() => {
        longHeld = null;
        if (objDrag?.moved || drag?.moved || placing || drawing || locked) return;
        // The press ends here: an armed drag that never moved does nothing,
        // a pan stops; the release later finds no press.
        endGesture();
        if (target.type === "item") selectItem(target.item);
        else if (target.type === "vertex") {
          selection = { part: target.part, edge: target.vertex };
          dormerSel = null;
          openSel = null;
          manualSel = null;
        }
        oncontext?.(target, client, snapPoint(local.x, local.y, false));
      }, LONG_MS),
    };
  }
  function noLongPress() {
    if (longHeld) clearTimeout(longHeld.timer);
    longHeld = null;
  }

  /** A press on a dormer, chimney, window, opening or hand-drawn shape. */
  function pressItem(e: PointerEvent, item: ItemRef) {
    if (placing || drawing) return;
    const b = button(e);
    if (b === 2) {
      selectItem(item);
      rcTarget = { type: "item", item };
      return;
    }
    if (b !== 0) return;
    e.stopPropagation();
    selectItem(item);
    // A second press on the same object within the double-click time opens
    // its properties (the dormer window, the object dialog). The pointer is
    // captured for the drag, so the element never sees a dblclick itself.
    const now = performance.now();
    const again = lastPress && lastPress.kind === item.kind && lastPress.index === item.index && now - lastPress.t < DOUBLE_MS;
    lastPress = again ? null : { ...item, t: now };
    if (again && onitemdbl) {
      onitemdbl(item);
      return;
    }
    startObjDrag(e, { type: "item", item });
  }
  /** The last press on an object, for the double-click. */
  let lastPress: (ItemRef & { t: number }) | null = null;
  const DOUBLE_MS = 450;

  /** A press inside a part's outline: drags the selected part, else selects it. */
  function pressPart(e: PointerEvent, part: number) {
    if (placing || drawing) return;
    const b = button(e);
    if (b === 2) {
      rcTarget = { type: "part", part };
      return;
    }
    if (b !== 0) return;
    e.stopPropagation();
    if (selection?.part !== part) {
      // The first press only selects, so panning across a roof never moves it.
      selection = { part, edge: 0 };
      dormerSel = null;
      openSel = null;
      manualSel = null;
      return;
    }
    startObjDrag(e, { type: "part", part });
  }

  /** A press on a wall: selects it. */
  function pressEdge(e: PointerEvent, part: number, edge: number) {
    if (placing || drawing) return;
    selection = { part, edge };
    dormerSel = null;
    openSel = null;
    manualSel = null;
    if (button(e) === 2) rcTarget = { type: "edge", part, edge };
    else e.stopPropagation();
  }

  function localPoint(e: PointerEvent | WheelEvent) {
    const r = svg!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  /** A press on the plan itself (also what a shape's press let through). */
  function pressPlan(e: PointerEvent) {
    const b = button(e);
    if (b === 2 && !placing && !drawing) {
      const p = localPoint(e);
      rc = { target: rcTarget ?? { type: "canvas" }, x: p.x, y: p.y, cx: e.clientX, cy: e.clientY };
      rcTarget = null;
      pan = { x: p.x, y: p.y, cx: center.x, cy: center.y };
      capture(e);
      return;
    }
    rcTarget = null;
    if (placing && e.button === 0) {
      const p = localPoint(e);
      const w = magnetPoint(p.x, p.y, e.altKey);
      onplace?.(w.x, w.y);
      return;
    }
    if (drawing && e.button === 0) {
      const now = performance.now();
      const p = localPoint(e);
      const first = drawing[0];
      // A double-click ends the footprint at the clicked corner and closes
      // it with a wall back to the first corner (as wall entry does in
      // the reference program); its first press already added the corner.
      const again = lastDrawAt && now - lastDrawPress < DOUBLE_MS && Math.hypot(lastDrawAt.x - p.x, lastDrawAt.y - p.y) < 6;
      lastDrawPress = now;
      lastDrawAt = again ? null : p;
      // Clicking the first point closes the footprint.
      if (first && drawing.length >= 3 && (again || Math.hypot(sx(first.x) - p.x, sy(first.y) - p.y) < 10)) {
        ondrawclose?.();
      } else {
        const w = magnetPoint(p.x, p.y, e.altKey);
        ondrawpoint?.(w.x, w.y);
      }
      return;
    }
    if (e.button === 1 || e.button === 2 || (e.button === 0 && e.target === svg)) {
      const p = localPoint(e);
      pan = { x: p.x, y: p.y, cx: center.x, cy: center.y };
      if (e.button === 0) {
        selection = null;
        dormerSel = null;
        openSel = null;
        manualSel = null;
      }
      capture(e);
    }
  }

  function onpointermove(e: PointerEvent) {
    if (longHeld && e.pointerId === longHeld.id && Math.hypot(e.clientX - longHeld.x, e.clientY - longHeld.y) > LONG_SLOP) noLongPress();
    if (otherPointer(e)) return;
    // The button went up where the plan could not see it (the system took
    // the pointer: another window, a native dialog) and no release, cancel
    // or lost capture came: the press is over, or the object would follow
    // the bare cursor until the next click (errors To check 60).
    if (gesture !== null && e.pointerType === "mouse" && e.buttons === 0) endGesture(e);
    const p = localPoint(e);
    cursorWorld = drag
      ? magnetPoint(p.x, p.y, e.altKey, drag)
      : drawing || placing
        ? magnetPoint(p.x, p.y, e.altKey)
        : snapPoint(p.x, p.y, e.altKey);
    if (!drag && !drawing && !placing && (guides.x !== null || guides.y !== null)) guides = { x: null, y: null };
    oncursor?.(cursorWorld);
    if (drag) {
      // A click on a corner does not move it; only a drag does.
      if (!drag.moved && Math.hypot(p.x - drag.sx, p.y - drag.sy) > DRAG_SLOP) drag.moved = true;
      if (drag.moved) onmove(drag.part, drag.vertex, cursorWorld.x, cursorWorld.y);
    } else if (objDrag) {
      if (!objDrag.moved && Math.hypot(p.x - objDrag.sx, p.y - objDrag.sy) > DRAG_SLOP) objDrag.moved = true;
      if (objDrag.moved) {
        const dx = +(cursorWorld.x - objDrag.wx).toFixed(3);
        const dy = +(cursorWorld.y - objDrag.wy).toFixed(3);
        if (dx !== objDrag.dx || dy !== objDrag.dy) {
          objDrag.dx = dx;
          objDrag.dy = dy;
          ondrag?.(objDrag.target, dx, dy);
        }
      }
    } else if (pan) {
      center = { x: pan.cx - (p.x - pan.x) / scale, y: pan.cy + (p.y - pan.y) / scale };
      if (Math.hypot(p.x - pan.x, p.y - pan.y) > 3) userView = true;
    }
  }

  function onpointerup(e: PointerEvent) {
    if (otherPointer(e)) return;
    endGesture(e);
  }

  function onwheel(e: WheelEvent) {
    e.preventDefault();
    const p = localPoint(e);
    const before = { x: wx(p.x), y: wy(p.y) };
    scale = Math.min(400, Math.max(4, scale * Math.exp(-e.deltaY * 0.0015)));
    userView = true;
    center = { x: center.x + before.x - wx(p.x), y: center.y + before.y - wy(p.y) };
  }

  /** A press on a corner: drags it. */
  function pressVertex(e: PointerEvent, part: number, vertex: number) {
    // While drawing or placing, a click on a corner is a click on the plan.
    if (placing || drawing) return;
    if (button(e) === 2) {
      selection = { part, edge: vertex };
      dormerSel = null;
      openSel = null;
      manualSel = null;
      rcTarget = { type: "vertex", part, vertex };
      return;
    }
    e.stopPropagation();
    if (!arm(e, { type: "vertex", part, vertex })) return;
    const p = localPoint(e);
    drag = { part, vertex, sx: p.x, sy: p.y, moved: false };
  }

  // When the last press of a drawing was, ms: the double-click that ends a
  // footprint on another building's wall is no corner for that wall
  // (review UX-001).
  let lastDrawPress = -Infinity;
  /** Where the last drawing press was, for the double-click that closes. */
  let lastDrawAt: { x: number; y: number } | null = null;

  /**
   * A double-click on a wall adds a corner on that wall: the pointer is
   * projected onto it and the distance from its start rounded to the grid
   * step, so the new corner lies on the wall. Rounded to the grid in x and
   * y, it lay up to a few cm off a slanted wall and bent it, which could
   * stop every roof (review ERR-002).
   */
  function edgeDblClick(e: MouseEvent, part: number, edge: number) {
    if (placing || drawing || performance.now() - lastDrawPress < 800) return;
    const es = project.parts[part]?.edges;
    if (!es) return;
    const a = es[edge].start;
    const b = es[(edge + 1) % es.length].start;
    const r = svg!.getBoundingClientRect();
    const q = onWall(a, b, { x: wx(e.clientX - r.left), y: wy(e.clientY - r.top) }, SNAP);
    if (q) oninsert(part, edge, q.x, q.y);
  }

  const lineColor: Record<string, string> = {
    eave: "var(--line-eave)",
    verge: "var(--line-verge)",
    ridge: "var(--line-ridge)",
    hip: "var(--line-hip)",
    valley: "var(--line-valley)",
    pult: "var(--line-pult)",
    knick: "var(--line-knick)",
    gorge: "var(--line-gorge)",
    abutment: "var(--line-abutment)",
    abutment_front: "var(--line-abutment)",
    abutment_back: "var(--line-abutment)",
    abutment_side: "var(--line-abutment)",
  };

  function edgeLength(part: number, i: number) {
    const es = outlinesOf[part].pts;
    const a = es[i];
    const b = es[(i + 1) % es.length];
    return Math.hypot(b.x - a.x, b.y - a.y);
  }

  /** The number and length a wall's dimension label shows ("2.3  8,00" with several buildings). */
  function dimText(part: number, i: number) {
    return { no: wallNumber(part, i, project.parts.length), len: lenText(edgeLength(part, i)) };
  }

  /** Screen position of a wall's dimension label: beside the wall, on the
   * outside of the part (`inside`: on the inside, used when the outside is
   * taken by a neighbouring building or another label, UX-018), so the wall
   * line never runs through the text. `t`: where along the wall (0.5 the
   * middle; off the middle when a roof line is there). */
  function dimPos(part: number, i: number, inside = false, t = 0.5) {
    const d = dimText(part, i);
    return besideWall(part, i, inside, t, `${d.no}  ${d.len}`.length * 3.3);
  }

  /** A label `halfW` px half wide beside wall `i`, `extra` px further off the wall than a wall length. */
  function besideWall(part: number, i: number, inside: boolean, t: number, halfW: number, extra = 0) {
    const o = outlinesOf[part];
    const a = o.pts[i];
    const b = o.pts[(i + 1) % o.pts.length];
    const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    const side = (o.ccw ? 1 : -1) * (inside ? -1 : 1);
    // Outward normal in world space, then flipped to screen space (y down).
    const nx = (side * (b.y - a.y)) / len;
    const ny = (-side * -(b.x - a.x)) / len;
    const off = 9 + Math.abs(nx) * halfW + Math.abs(ny) * 4 + extra;
    return { x: sx(a.x) + (sx(b.x) - sx(a.x)) * t + nx * off, y: sy(a.y) + (sy(b.y) - sy(a.y)) * t + ny * off, w: halfW * 2 + 4 };
  }

  const NO_LINES: Seg[] = [];

  /** How a face label is written: pitch and area, pitch only, or not at all (too small). */
  function tagMode(tag: (typeof tags)[number]): "full" | "pitch" | null {
    const along = tag.along * scale;
    const across = tag.across * scale;
    if (!(along > textPx(pitchText(tag.pitch_deg), 16) && across > 20)) return null;
    return along > textPx(areaText(tag.area), 12.5) && across > 44 ? "full" : "pitch";
  }

  /**
   * Which wall dimensions and face labels are drawn: none may overlap one
   * drawn before it (wall lengths first, part by part, then the faces), so
   * buildings drawn into each other keep a readable plan (UX-012).
   */
  const labels = $derived.by(() => {
    // The selected object's distances come first: they are what the user
    // is placing by; the other labels keep clear of them (UI-209).
    const placeDims = placed
      ? [
          { l: placed.eave_line, v: placed.from_eave, key: "eave", tip: t("place.fromEaveHint") },
          { l: placed.left_line, v: placed.from_left, key: "left", tip: t("place.fromLeftHint") },
          { l: placed.right_line, v: placed.from_right, key: "right", tip: t("place.fromRightHint") },
        ].map((d) => {
          const g = dimLine(d.l[0], d.l[1]);
          return { ...d, g, text: mmText(d.v), spots: placeDimSpots(g, mmText(d.v), d.key === "eave") };
        })
      : [];
    // Only what the window shows is placed (with a margin, so a label half
    // in view is still there): a job of 200 buildings placed every label of
    // every building at each zoom and pan step (UX-602).
    const MARGIN = 160;
    const view: Box = { x0: -MARGIN, y0: -MARGIN, x1: width + MARGIN, y1: height + MARGIN };
    const inView = (x0: number, y0: number, x1: number, y1: number) => x1 >= view.x0 && x0 <= view.x1 && y1 >= view.y0 && y0 <= view.y1;
    // Each wall length outside its building first, else inside; a side
    // whose label would sit on another building is not used, so two
    // buildings side by side keep their lengths apart (UX-018).
    const outlines = outlinesOf;
    // Tests run by this pass (outline, wall, line and label comparisons),
    // shown as `data-label-work` for the guard of UX-602.
    const work = { overlaps: 0, lines: 0 };
    const onOther = (pi: number, x: number, y: number) => {
      const w = { x: wx(x), y: wy(y) };
      return outlines.some((o, k) => k !== pi && o.pts.length >= 3 && w.x >= o.x0 && w.x <= o.x1 && w.y >= o.y0 && w.y <= o.y1 && (work.lines += o.pts.length) > 0 && insidePolygon(w, o.pts));
    };
    const dims = outlines.map((o, pi) =>
      o.pts.map((a, i) => {
        if (!showDims || !(scale * edgeLength(pi, i) > 46)) return null;
        const b = o.pts[(i + 1) % o.pts.length];
        const [ax, ay, bx, by] = [sx(a.x), sy(a.y), sx(b.x), sy(b.y)];
        if (!inView(Math.min(ax, bx), Math.min(ay, by), Math.max(ax, bx), Math.max(ay, by))) return null;
        // Outside, inside, then off the middle where a roof line runs
        // through the middle (a ridge meeting the wall), outside first.
        const at: [boolean, number][] = [[false, 0.5], [true, 0.5], [false, 0.35], [false, 0.65], [true, 0.35], [true, 0.65]];
        return at.map(([inside, t]) => dimPos(pi, i, inside, t)).map((p) => (onOther(pi, p.x, p.y) ? null : { ...p, box: rotatedBox(p.x, p.y, p.w, DIM_H, 0) }));
      }),
    );
    // What the AI changed on a wall, beside it, a row further off than its
    // length (UX-017); placed before the lengths, it is what the review is for.
    const notes = [...wallNotes].flatMap(([key, text]) => {
      const [pi, i] = key.split(":").map(Number);
      if (!outlines[pi]?.pts[i]) return [];
      const halfW = textPx(text, 12) / 2 + 3;
      // The wall's direction on screen, kept readable: a note may run along
      // the wall, so beside a steep wall it fits outside the roof instead of
      // lying across the ridge and the hips inside it (UX-007).
      const es = outlines[pi].pts;
      const [a, b] = [es[i], es[(i + 1) % es.length]];
      let deg = (Math.atan2(sy(b.y) - sy(a.y), sx(b.x) - sx(a.x)) * 180) / Math.PI;
      if (deg > 90) deg -= 180;
      if (deg <= -90) deg += 180;
      // [inside, where along the wall, along the wall]
      const at: [boolean, number, boolean][] = [
        [false, 0.5, false], [false, 0.5, true], [false, 0.3, false], [false, 0.7, false], [false, 0.3, true], [false, 0.7, true],
        [true, 0.5, true], [true, 0.3, true], [true, 0.7, true], [true, 0.5, false], [true, 0.3, false], [true, 0.7, false],
      ];
      const cands = at.map(([inside, t, along]) => {
        const p = along ? besideWall(pi, i, inside, t, 0, DIM_H / 2 + DIM_H) : besideWall(pi, i, inside, t, halfW, DIM_H);
        const rot = along ? deg : 0;
        const box = rotatedBox(p.x, p.y, halfW * 2, DIM_H, rot);
        // A note cut by the edge of the plan is no use: take one inside.
        return box.x0 >= 2 && box.y0 >= 2 && box.x1 <= width - 2 && box.y1 <= height - 2 ? { x: p.x, y: p.y, rot, text, box } : null;
      });
      return [cands];
    });
    // A face none of whose label spots is in view gets no label this step.
    const tagInView = (tag: (typeof tags)[number]) => tag.spots.some((p) => inView(sx(p.x), sy(p.y), sx(p.x), sy(p.y)));
    const modes = showDims && !faceLabels.length ? tags.map((tag) => (tagInView(tag) ? tagMode(tag) : null)) : tags.map(() => null);
    const flat = dims.flat();
    // Wall lengths keep off the roof lines inside the roof (the eave and
    // verge run along the walls, every wall length sits by them); face
    // labels keep off the walls, where the wall lengths are. Both are
    // looked up by screen box (planlabels.ts `segIndex`), in view only.
    const inner = segIndex(innerLines.map((l): Seg => [sx(l[0]), sy(l[1]), sx(l[2]), sy(l[3])]), view);
    const wallSegs: Seg[] = [];
    const wallPart: number[] = [];
    outlines.forEach((o, pi) =>
      o.pts.forEach((p, k) => {
        const q = o.pts[(k + 1) % o.pts.length];
        wallSegs.push([sx(p.x), sy(p.y), sx(q.x), sy(q.y)]);
        wallPart.push(pi);
      }),
    );
    const walls = segIndex(wallSegs, view);
    // A face label that does not fit with its area falls back to the pitch
    // alone; both may move off the centre (up or down the slope), and never
    // sit across another building's walls, e.g. one drawn inside this roof (UI-005).
    const tagBoxes = tags.map((tag, k) => {
      const m = modes[k];
      if (!m) return [];
      const clear = (b: ReturnType<typeof rotatedBox>) => (walls.nearAt(b).some((i) => (work.lines++, wallPart[i] !== tag.part && segCrossesBox(wallSegs[i], b))) ? null : b);
      const pw = textPx(pitchText(tag.pitch_deg), 16);
      const fw = Math.max(pw, textPx(areaText(tag.area), 12.5));
      return tag.spots.flatMap((p) => {
        const pitchBox = clear(rotatedBox(sx(p.x), sy(p.y), pw, TAG_PITCH_H, -tag.angle_deg));
        return m === "pitch" ? [null, pitchBox] : [clear(rotatedBox(sx(p.x), sy(p.y), fw, TAG_FULL_H, -tag.angle_deg)), pitchBox];
      });
    });
    const P = placeDims.length;
    const chosen = chooseLabels(
      [...placeDims.map((d) => d.spots.map((sp) => sp.box)), ...notes.map((n) => n.map((c) => c?.box ?? null)), ...flat.map((d) => (d ?? []).map((c) => c?.box ?? null)), ...tagBoxes],
      (i) => (i < P ? NO_LINES : i < P + notes.length + flat.length ? inner : walls),
      work,
    );
    const placeShown = placeDims.map((d, j) => ({ ...d, at: (chosen[j] >= 0 ? d.spots[chosen[j]] : null) as DimSpot | null }));
    const noteShown = notes.flatMap((n, j) => (chosen[P + j] >= 0 && n[chosen[P + j]] ? [n[chosen[P + j]]!] : []));
    let k = P + notes.length;
    const dimShown = dims.map((row) =>
      row.map((d) => {
        const j = chosen[k++];
        return d && j >= 0 ? d[j] : null;
      }),
    );
    const tagShown = tags.map((tag, j) => {
      const c = chosen[k + j];
      if (c < 0 || !modes[j]) return null;
      const at = tag.spots[Math.floor(c / 2)];
      return { mode: c % 2 === 0 ? modes[j]! : ("pitch" as const), x: at.x, y: at.y };
    });
    return { dims: dimShown, tags: tagShown, notes: noteShown, place: placeShown, work: work.lines + work.overlaps };
  });
</script>

<div class="wrap" bind:clientWidth={width} bind:clientHeight={height}>
  <svg
    bind:this={svg}
    {width}
    {height}
    role="application"
    aria-label={t("editor.plan")}
    data-label-work={labels.work}
    onpointerdown={(ev) => press(ev, { on: "plan" })}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
    onlostpointercapture={(e) => e.pointerId === gesture && endGesture(e)}
    {onwheel}
    onpointerleave={() => {
      cursorWorld = null;
      oncursor?.(null);
    }}
    class:drawing={!!drawing || placing}
    oncontextmenu={(e) => e.preventDefault()}
  >
    {#each gridLines as g}
      <path d={g.d} class:major={g.major} class="grid" />
    {/each}

    {#each project.parts as part, pi}
      <path
        class="fill"
        class:movable={selection?.part === pi}
        d={"M" + part.edges.map((e) => `${sx(e.start.x)},${sy(e.start.y)}`).join("L") + "Z"}
        role="button"
        tabindex="-1"
        aria-label={partLabel(part.name, pi)}
        onpointerdown={(ev) => press(ev, { on: "part", part: pi })}
      />
    {/each}

    {#each materialFaces as f}
      <path class="mat-face" style:fill={`color-mix(in srgb, var(${materialToken(f.material ?? 1)}) 55%, transparent)`}
        d={"M" + f.polygon.map((p) => `${sx(p[0])},${sy(p[1])}`).join("L") + "Z"} />
    {/each}

    <!-- Roof lines answer the pointer under the walls, so a wall on the
         same line (no overhang) keeps its own press. -->
    {#if !placing && !drawing}
      {#each result?.parts ?? [] as rp, pi}
        {#each rp.geometry.lines as l, li}
          <line class="line-hit" x1={sx(l.a[0])} y1={sy(l.a[1])} x2={sx(l.b[0])} y2={sy(l.b[1])}
            data-line={`${pi}:${li}`} role="button" tabindex="-1" aria-label={t("lineinfo.title")}
            onpointerenter={() => { if (hovering()) onlinehover?.({ part: pi, index: li }); }}
            onpointerleave={() => onlinehover?.(null)}
            onpointerdown={(ev) => press(ev, { on: "part", part: pi, line: li })} />
        {/each}
      {/each}
    {/if}

    {#each linePieces as lines}
      {#each lines as l}
        <!-- Eaves are drawn over the walls below (UX-004). -->
        {#if l.kind !== "eave"}
          {#each l.pieces as piece}
            <line
              x1={sx(piece.a[0])}
              y1={sy(piece.a[1])}
              x2={sx(piece.b[0])}
              y2={sy(piece.b[1])}
              stroke={lineColor[l.kind]}
              class="roofline"
              class:covered={piece.hidden}
            />
          {/each}
        {/if}
      {/each}
    {/each}

    {#if ghost}
      <g class="ghost" aria-hidden="true">
        {#each ghost.shapes as pts}
          <path d={"M" + pts.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"} />
        {/each}
        {#each ghost.lines as [a, b]}
          <line x1={sx(a.x)} y1={sy(a.y)} x2={sx(b.x)} y2={sy(b.y)} />
        {/each}
      </g>
    {/if}
    {#if mark && "polygon" in mark}
      <path class="mark" d={"M" + mark.polygon.map((p) => `${sx(p[0])},${sy(p[1])}`).join("L") + "Z"} />
    {:else if mark && "line" in mark}
      <line class="mark-line" x1={sx(mark.line[0][0])} y1={sy(mark.line[0][1])} x2={sx(mark.line[1][0])} y2={sy(mark.line[1][1])} />
    {/if}
    {#if showDims && !faceLabels.length}
      {#each tags as tag, k}
        {@const shown = labels.tags[k]}
        {#if shown}
          <g class="face-tag" transform="translate({sx(shown.x)} {sy(shown.y)}) rotate({-tag.angle_deg})">
            {#if shown.mode === "full"}
              <text class="pitch" y="-8">{pitchText(tag.pitch_deg)}</text>
              <text class="area" y="9">{areaText(tag.area)}</text>
            {:else}
              <text class="pitch">{pitchText(tag.pitch_deg)}</text>
            {/if}
          </g>
        {/if}
      {/each}
    {/if}
    {#each faceLabels as l}
      <text class="face-no" x={sx(l.x)} y={sy(l.y)}>{l.text}</text>
    {/each}

    {#each project.parts as part, pi}
      {#each part.edges as e, i}
        {@const b = part.edges[(i + 1) % part.edges.length].start}
        {@const sel = selection?.part === pi && selection?.edge === i}
        {@const changed = highlight.has(`${pi}:${i}`)}
        <g class="edge" class:gable={isGable(e)} class:sel class:changed>
          {#if changed}
            <line class="glow" x1={sx(e.start.x)} y1={sy(e.start.y)} x2={sx(b.x)} y2={sy(b.y)} />
          {/if}
          {#if sel}
            <line class="sel-halo" x1={sx(e.start.x)} y1={sy(e.start.y)} x2={sx(b.x)} y2={sy(b.y)} />
          {/if}
          <line class="hit" x1={sx(e.start.x)} y1={sy(e.start.y)} x2={sx(b.x)} y2={sy(b.y)}
            role="button" tabindex="-1" aria-label={t("plan.edge", { n: i + 1 })}
            onpointerdown={(ev) => press(ev, { on: "edge", part: pi, edge: i })}
            ondblclick={(ev) => edgeDblClick(ev, pi, i)} />
          <line class="wall" x1={sx(e.start.x)} y1={sy(e.start.y)} x2={sx(b.x)} y2={sy(b.y)} />
          {#if labels.dims[pi]?.[i]}
            {@const lp = labels.dims[pi][i]!}
            {@const dt = dimText(pi, i)}
            <text x={lp.x} y={lp.y} class="dim edge-dim"><tspan class="no">{dt.no}</tspan> <tspan class="len" dx="4">{dt.len}</tspan></text>
          {/if}
        </g>
      {/each}
      {#each part.edges as e, i}
        <circle
          class="vertex"
          cx={sx(e.start.x)}
          cy={sy(e.start.y)}
          r="5"
          role="button"
          tabindex="-1"
          aria-label={t("plan.vertex", { n: i + 1 })}
          onpointerdown={(ev) => press(ev, { on: "vertex", part: pi, vertex: i })}
        />
      {/each}
    {/each}
    <!-- The eave (T) at the overhang, where the results measure it; over the
         walls, so with no overhang the wall shows the eave too (UX-004). -->
    {#each linePieces as lines}
      {#each lines as l}
        {#if l.kind === "eave"}
          {#each l.pieces as piece}
            <line
              x1={sx(piece.a[0])}
              y1={sy(piece.a[1])}
              x2={sx(piece.b[0])}
              y2={sy(piece.b[1])}
              stroke={lineColor.eave}
              class="roofline eave"
              class:covered={piece.hidden}
            />
          {/each}
        {/if}
      {/each}
    {/each}
    {#each markedLines as l}
      <g class="line-mark" aria-hidden="true">
        <line class="edge-hl" x1={sx(l.a[0])} y1={sy(l.a[1])} x2={sx(l.b[0])} y2={sy(l.b[1])} />
        <line class="core-hl" x1={sx(l.a[0])} y1={sy(l.a[1])} x2={sx(l.b[0])} y2={sy(l.b[1])} />
        <circle cx={sx(l.a[0])} cy={sy(l.a[1])} r="3.5" />
        <circle cx={sx(l.b[0])} cy={sy(l.b[1])} r="3.5" />
      </g>
    {/each}
    {#each project.dormers as d, di}
      {@const fs = d.edges.find((x) => x.direction_rad == null)?.start ?? d.edges[0]?.start}
      {#if fs}
        <g class="dormer" class:sel={dormerSel === di} class:changed={highlightItems.has(`dormer:${di}`)}>
          {#each dormerFaces[di] ?? [] as poly}
            <path class="dormer-area" d={"M" + poly.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
              role="button" tabindex="-1" aria-label={t("plan.dormer", { n: di + 1 })}
              onpointerdown={(ev) => press(ev, { on: "item", item: { kind: "dormer", index: di } })} />
          {/each}
          {#each d.edges as x, k}
            {#if x.direction_rad != null}
              {@const s0 = k === 0 ? -0.8 : 0.8}
              <line class="cheek" x1={sx(x.start.x)} y1={sy(x.start.y)}
                x2={sx(x.start.x + Math.cos(x.direction_rad) * s0)} y2={sy(x.start.y + Math.sin(x.direction_rad) * s0)} />
            {/if}
          {/each}
          <line class="hit" x1={sx(fs.x)} y1={sy(fs.y)} x2={sx(d.end.x)} y2={sy(d.end.y)}
            role="button" tabindex="-1" aria-label={t("plan.dormer", { n: di + 1 })}
            onpointerdown={(ev) => press(ev, { on: "item", item: { kind: "dormer", index: di } })} />
          <line class="front" x1={sx(fs.x)} y1={sy(fs.y)} x2={sx(d.end.x)} y2={sy(d.end.y)} />
        </g>
      {/if}
    {/each}
    {#each project.chimneys as c, ci}
      <path class="opening chimney" class:sel={openSel?.kind === "chimney" && openSel.index === ci} class:changed={highlightItems.has(`chimney:${ci}`)}
        d={"M" + c.corners.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
        role="button" tabindex="-1" aria-label={t("opening.chimneyN", { n: ci + 1 })}
        onpointerdown={(ev) => press(ev, { on: "item", item: { kind: "chimney", index: ci } })} />
    {/each}
    {#each project.roof_openings ?? [] as c, hi}
      <path class="opening hole" class:sel={openSel?.kind === "hole" && openSel.index === hi} class:changed={highlightItems.has(`roof_opening:${hi}`)}
        d={"M" + c.corners.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
        role="button" tabindex="-1" aria-label={t("opening.holeN", { n: hi + 1 })}
        onpointerdown={(ev) => press(ev, { on: "item", item: { kind: "hole", index: hi } })} />
    {/each}
    {#each project.windows ?? [] as w, wi}
      <path class="opening window" class:sel={openSel?.kind === "window" && openSel.index === wi} class:changed={highlightItems.has(`window:${wi}`)}
        d={"M" + windowOutline(result, w).map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
        role="button" tabindex="-1" aria-label={t("opening.windowN", { n: wi + 1 })}
        onpointerdown={(ev) => press(ev, { on: "item", item: { kind: "window", index: wi } })} />
    {/each}
    {#each project.manual_faces ?? [] as f, fi}
      <path class="manual-face" class:sel={manualSel?.kind === "face" && manualSel.index === fi} class:changed={highlightItems.has(`manual_face:${fi}`)}
        d={"M" + f.points.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
        role="button" tabindex="-1" aria-label={t("manual.faceN", { n: fi + 1 })}
        onpointerdown={(ev) => press(ev, { on: "item", item: { kind: "face", index: fi } })} />
      {#if showDims && f.points.length >= 3}
        <!-- Says what the outline is, so it is not taken for a building without a roof. -->
        <text class="manual-tag" x={sx(f.points.reduce((a, p) => a + p.x, 0) / f.points.length)} y={sy(f.points.reduce((a, p) => a + p.y, 0) / f.points.length)}>{t("manual.faceN", { n: fi + 1 })} · {pitchText(f.pitch_deg)}</text>
      {/if}
    {/each}
    {#each project.manual_lines ?? [] as l, li}
      {@const sel = manualSel?.kind === "line" && manualSel.index === li}
      <g class="manual-line" class:sel class:changed={highlightItems.has(`manual_line:${li}`)}>
        <line class="hit" x1={sx(l.a[0])} y1={sy(l.a[1])} x2={sx(l.b[0])} y2={sy(l.b[1])}
          role="button" tabindex="-1" aria-label={t("manual.lineN", { n: li + 1 })}
          onpointerdown={(ev) => press(ev, { on: "item", item: { kind: "line", index: li } })} />
        <line class="stroke" class:measure={!kindOf(l.code)} x1={sx(l.a[0])} y1={sy(l.a[1])} x2={sx(l.b[0])} y2={sy(l.b[1])}
          stroke={kindOf(l.code) ? lineColor[kindOf(l.code)!] : undefined} />
        {#if showDims}
          <text class="dim" x={(sx(l.a[0]) + sx(l.b[0])) / 2} y={(sy(l.a[1]) + sy(l.b[1])) / 2} dy="-6">{l.label ? `${l.label} ` : ""}{lenText(lineLength(l))}</text>
        {/if}
      </g>
    {/each}
    {#if manualDraft.length}
      <path class="draft" d={"M" + manualDraft.map((p) => `${sx(p[0])},${sy(p[1])}`).join("L") +
        (cursorWorld ? `L${sx(cursorWorld.x)},${sy(cursorWorld.y)}` : "")} />
      {#each manualDraft as p, i}
        <circle class="draft-pt" class:first={i === 0} cx={sx(p[0])} cy={sy(p[1])} r="4" />
      {/each}
    {/if}
    {#if guides.x !== null}
      <path class="guide" d="M{sx(guides.x)},0V{height}" />
    {/if}
    {#if guides.y !== null}
      <path class="guide" d="M0,{sy(guides.y)}H{width}" />
    {/if}
    {#if placed}
      <!-- From the eave (along the slope) and from both sides of the face:
           the selected object's dimension lines (core edit::position). -->
      <g class="place-dims" data-testid="place-dims">
        {#each labels.place as d (d.key)}
          {@const g = d.g}
          <g data-dim={d.key}>
            <title>{d.tip}</title>
            <line x1={g.x1} y1={g.y1} x2={g.x2} y2={g.y2} />
            <path d={g.ticks} />
            <!-- Written where no other label is; left out when there is no room (the bar has the figure, UI-209).
                 The anchor is a style: the .dim rule's text-anchor would override the attribute. -->
            {#if d.at}<text class="dim place-dim" x={d.at.x} y={d.at.y} style:text-anchor={d.at.anchor}>{d.text}</text>{/if}
          </g>
        {/each}
      </g>
    {/if}
    {#if placing && cursorWorld}
      <path class="crosshair" d="M{sx(cursorWorld.x) - 9},{sy(cursorWorld.y)}h18M{sx(cursorWorld.x)},{sy(cursorWorld.y) - 9}v18" />
    {/if}
    {#if drawing}
      {#if drawing.length}
        <path
          class="draft"
          d={"M" + drawing.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") +
            (cursorWorld ? `L${sx(cursorWorld.x)},${sy(cursorWorld.y)}` : "")}
        />
        {#if drawing.length >= 2}
          <!-- The wall that closing adds, back to the first corner. -->
          {@const first = drawing[0]}
          {@const end = cursorWorld ?? drawing[drawing.length - 1]}
          {@const c = Math.hypot(first.x - end.x, first.y - end.y)}
          {#if c > 0.01}
            <path class="draft-close" data-testid="draft-close" d="M{sx(end.x)},{sy(end.y)}L{sx(first.x)},{sy(first.y)}" />
            <text class="dim draft-dim close" x={(sx(first.x) + sx(end.x)) / 2} y={(sy(first.y) + sy(end.y)) / 2} dy="14">{lenText(c)}</text>
          {/if}
        {/if}
        <!-- Every wall drawn so far keeps its length, as the closed footprint does (UX-014). -->
        {#each draftWalls(drawing) as w}
          {@const ax = sx(w.a.x)}
          {@const ay = sy(w.a.y)}
          {@const bx = sx(w.b.x)}
          {@const by = sy(w.b.y)}
          {@const sl = Math.hypot(bx - ax, by - ay) || 1}
          {#if sl > 30}
            <text class="dim draft-dim wall-len" data-testid="draft-wall-dim"
              x={(ax + bx) / 2 + ((by - ay) / sl) * 12} y={(ay + by) / 2 - ((bx - ax) / sl) * 12}>{lenText(w.length)}</text>
          {/if}
        {/each}
        {#each drawing as p, i}
          <circle class="draft-pt" class:first={i === 0} cx={sx(p.x)} cy={sy(p.y)} r={i === 0 && drawing.length >= 3 ? 7 : 4} />
        {/each}
        {#if cursorWorld}
          {@const last = drawing[drawing.length - 1]}
          {@const d = Math.hypot(cursorWorld.x - last.x, cursorWorld.y - last.y)}
          {#if d > 0.01}
            <text class="dim draft-dim" x={(sx(last.x) + sx(cursorWorld.x)) / 2} y={(sy(last.y) + sy(cursorWorld.y)) / 2} dy="-8">{lenText(d)}</text>
          {/if}
        {/if}
      {/if}
      {#if cursorWorld}
        <path class="crosshair" d="M{sx(cursorWorld.x) - 9},{sy(cursorWorld.y)}h18M{sx(cursorWorld.x)},{sy(cursorWorld.y) - 9}v18" />
      {/if}
    {/if}
    {#each labels.notes as n}
      <text class="wall-note" data-testid="wall-note" x={n.x} y={n.y} transform={n.rot ? `rotate(${n.rot} ${n.x} ${n.y})` : undefined}>{n.text}</text>
    {/each}
  </svg>
  <!-- A placing tool says what to click in the bottom bar (PlaceBar, UX-012). -->
  {#if !placing && !drawing}<div class="hint" data-testid="plan-hint">{moving ? t("plan.hintDrag") : touchHint ? t("plan.hintTouch") : t("plan.hint")}</div>{/if}
</div>

<style>
  .wrap {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--surface);
  }
  svg {
    display: block;
    cursor: grab;
    touch-action: none;
  }
  .grid {
    stroke: var(--plan-grid);
    stroke-width: 1;
  }
  .grid.major {
    stroke: color-mix(in srgb, var(--plan-grid) 60%, var(--text-3));
  }
  .dormer .front {
    stroke: var(--text);
    stroke-width: 3;
    stroke-linecap: round;
    pointer-events: none;
  }
  .dormer .cheek {
    stroke: var(--text-2);
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
  }
  .dormer-area {
    fill: transparent;
    stroke: none;
    cursor: pointer;
  }
  .dormer .hit {
    stroke: transparent;
    stroke-width: 14;
    cursor: pointer;
  }
  .dormer.sel .front {
    stroke: var(--accent);
  }
  .fill {
    fill: var(--plan-fill);
  }
  .fill.movable {
    cursor: move;
    fill: color-mix(in srgb, var(--focus) 7%, var(--plan-fill));
  }
  .opening {
    stroke: var(--text-2);
    stroke-width: 1.5;
    cursor: pointer;
  }
  .opening.sel,
  .dormer.sel .hit,
  .manual-face.sel,
  .manual-line.sel .hit {
    cursor: move;
  }
  .opening.chimney {
    fill: color-mix(in srgb, var(--text-3) 35%, transparent);
  }
  .opening.hole {
    fill: var(--surface);
    stroke-dasharray: 4 3;
  }
  .opening.window {
    fill: color-mix(in srgb, var(--window) 28%, transparent);
  }
  .opening.sel {
    stroke: var(--accent);
    stroke-width: 2.5;
  }
  .mat-face {
    pointer-events: none;
  }
  .line-hit {
    stroke: transparent;
    stroke-width: 10;
    stroke-linecap: round;
    cursor: pointer;
  }
  .line-mark {
    pointer-events: none;
  }
  .line-mark .edge-hl {
    stroke: var(--line-hl-edge);
    stroke-width: 7;
    stroke-linecap: round;
  }
  .line-mark .core-hl {
    stroke: var(--line-hl);
    stroke-width: 4;
    stroke-linecap: round;
  }
  .line-mark circle {
    fill: var(--line-hl);
    stroke: var(--line-hl-edge);
    stroke-width: 1.5;
  }
  .mark {
    fill: color-mix(in srgb, var(--accent) 22%, transparent);
    stroke: var(--accent);
    stroke-width: 2;
    pointer-events: none;
  }
  .mark-line {
    stroke: var(--accent);
    stroke-width: 4;
    stroke-linecap: round;
    pointer-events: none;
  }
  .face-tag text {
    font-family: inherit;
    font-variant-numeric: tabular-nums;
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 4px;
    stroke-linejoin: round;
  }
  .face-tag .pitch {
    fill: var(--text);
    font-size: 16px;
    font-weight: 700;
  }
  .face-tag .area {
    fill: var(--text-2);
    font-size: 12.5px;
    font-weight: 600;
  }
  .face-no {
    fill: var(--text-2);
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11px;
    font-weight: 700;
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
  }
  /* Neutral, like an annotation: brick red is the building fill and the
     selection, so a red outline read as a building whose roof failed. */
  .manual-face {
    fill: color-mix(in srgb, var(--text-3) 12%, transparent);
    stroke: var(--text-2);
    stroke-width: 1.2;
    stroke-dasharray: 2 3;
    cursor: pointer;
  }
  .manual-face.sel {
    stroke: var(--accent);
    stroke-width: 2.5;
    stroke-dasharray: none;
  }
  .manual-tag {
    font-size: 11.5px;
    fill: var(--text-2);
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
  }
  .manual-line .stroke {
    stroke-width: 2.2;
    stroke-dasharray: 7 3 1.5 3;
    pointer-events: none;
  }
  .manual-line .stroke.measure {
    stroke: var(--text-2);
    stroke-width: 1.2;
  }
  .manual-line .hit {
    stroke: transparent;
    stroke-width: 12;
    cursor: pointer;
  }
  .manual-line.sel .stroke {
    stroke: var(--focus);
    stroke-width: 3.5;
  }
  /* A press on a ridge, hip or valley goes to the line's wider hit line
     below: its details, and a press on the building under it (UI-701). */
  .roofline {
    stroke-width: 1.8;
    stroke-linecap: round;
    pointer-events: none;
  }
  .roofline.eave {
    pointer-events: none;
  }
  /* Under a higher roof (joined buildings): still there, but not on top. */
  .roofline.covered {
    stroke-width: 1.4;
    stroke-dasharray: 4 4;
    stroke-linecap: butt;
    opacity: 0.55;
  }
  /* Walls in a neutral colour: blue is the eave line at the overhang, so the
     blue lines in the plan add up to the eave length in the results (UX-004).
     A gable wall stays dashed. */
  .edge .wall {
    stroke: var(--text-2);
    stroke-width: 3;
    stroke-linecap: round;
    pointer-events: none;
  }
  .edge.gable .wall {
    stroke-dasharray: 8 5;
  }
  .edge .glow {
    stroke: var(--ai);
    stroke-width: 12;
    stroke-linecap: round;
    opacity: 0.35;
    pointer-events: none;
    animation: pulse 1.6s ease-in-out infinite;
  }
  .edge.changed .wall {
    stroke: var(--ai);
    stroke-dasharray: none;
  }
  /* Items the AI added or changed: same orange as changed walls. */
  .opening.changed,
  .manual-face.changed {
    stroke: var(--ai);
    stroke-width: 2.5;
    animation: pulse-stroke 1.6s ease-in-out infinite;
  }
  .dormer.changed .front,
  .dormer.changed .cheek,
  .manual-line.changed .stroke {
    stroke: var(--ai);
  }
  @keyframes pulse-stroke {
    50% {
      stroke-opacity: 0.45;
    }
  }
  /* The state before the pending AI changes. */
  .ghost path,
  .ghost line {
    fill: none;
    stroke: var(--ai-strong);
    stroke-width: 1.4;
    stroke-dasharray: 5 4;
    opacity: 0.7;
    pointer-events: none;
  }
  @keyframes pulse {
    50% {
      opacity: 0.12;
    }
  }
  svg.drawing {
    cursor: crosshair;
  }
  .draft {
    fill: color-mix(in srgb, var(--accent) 6%, transparent);
    stroke: var(--accent);
    stroke-width: 2.5;
    stroke-dasharray: 6 4;
    pointer-events: none;
  }
  .draft-close {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    stroke-dasharray: 2 4;
    opacity: 0.7;
    pointer-events: none;
  }
  .draft-pt {
    fill: var(--surface);
    stroke: var(--accent);
    stroke-width: 2;
    pointer-events: none;
  }
  .draft-pt.first {
    fill: var(--accent);
  }
  .guide {
    stroke: var(--focus);
    stroke-width: 1;
    stroke-dasharray: 3 3;
    opacity: 0.7;
    pointer-events: none;
  }
  .crosshair {
    stroke: var(--text-2);
    stroke-width: 1;
    pointer-events: none;
  }
  .edge-dim {
    dominant-baseline: central;
  }
  .place-dims line,
  .place-dims path {
    stroke: var(--accent);
    stroke-width: 1.2;
    fill: none;
    pointer-events: none;
  }
  .place-dims line {
    stroke-dasharray: 5 3;
  }
  .place-dim {
    fill: var(--accent);
    font-weight: 600;
    dominant-baseline: central;
  }
  /* What the AI changed on a wall (UX-017): AI colour, on a halo so it
     reads over grid and roof lines. */
  .wall-note {
    fill: var(--ai-strong);
    font-size: 12px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 4px;
    stroke-linejoin: round;
  }
  .dim .no {
    font-weight: 700;
    fill: var(--text);
  }
  /* The selected wall must not look like an eave (also blue): a wide
     selection halo with the wall drawn in the text colour on top, and its
     number and length in bold (UX-007). */
  .edge .sel-halo {
    stroke: var(--focus);
    stroke-width: 13;
    stroke-linecap: round;
    opacity: 0.3;
    pointer-events: none;
  }
  .edge.sel .wall {
    stroke: var(--text);
    stroke-width: 3.5;
    stroke-dasharray: none;
  }
  .edge.sel .edge-dim {
    font-weight: 700;
    fill: var(--text);
  }
  /* SVG collapses the space between number and length, and the bold length
     of the selected wall ran into its number ("1 12,00" read as "112,00",
     UI-001): a real gap, and the length in normal weight. */
  .edge.sel .edge-dim .len {
    font-weight: 500;
  }
  svg :global(*:focus) {
    outline: none;
  }
  .edge .hit {
    stroke: transparent;
    stroke-width: 14;
    cursor: pointer;
  }
  .edge:hover .wall {
    stroke-width: 5;
  }
  .dim {
    fill: var(--text-2);
    font-size: 11px;
    text-anchor: middle;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
    pointer-events: none;
  }
  .vertex {
    fill: var(--surface);
    stroke: var(--text);
    stroke-width: 1.6;
    cursor: move;
  }
  .vertex:hover {
    fill: var(--accent);
    stroke: var(--accent);
  }
  .hint {
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 8px;
    width: fit-content;
    max-width: calc(100% - 20px);
    padding: 3px 8px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--text-2);
    pointer-events: none;
  }
</style>
