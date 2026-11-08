<script lang="ts">
  import { untrack } from "svelte";
  import type { CalcResult, PlanLabel, PlanMark, Point2, Project, Selection } from "./types";
  import { isGable } from "./types";
  import { t } from "./i18n.svelte";
  import { windowOutline, type OpeningSel } from "./openings";
  import { codeKind, lineLength, type ManualSel } from "./manual";
  import type { Ghost } from "./changes";
  import type { ItemRef, PlanTarget } from "./items";
  import { fitScale, gridValues } from "./grid";
  import { faceTags } from "./facelabels";
  import { numberLocale } from "./i18n.svelte";

  let {
    project,
    result,
    selection = $bindable(),
    onmove,
    onmoveend,
    oninsert,
    highlight = new Set<string>(),
    highlightItems = new Set<string>(),
    ghost = null,
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
    oncontext,
  }: {
    project: Project;
    result: CalcResult | null;
    selection: Selection;
    onmove: (part: number, vertex: number, x: number, y: number) => void;
    /** A corner drag ended (the corner moved is given). */
    onmoveend: (part: number, vertex: number) => void;
    oninsert: (part: number, edge: number, x: number, y: number) => void;
    /** "part:edge" keys of edges changed by the latest AI edit. */
    highlight?: Set<string>;
    /** "field:item" keys (dormer:0, chimney:1, …) of items changed by the AI. */
    highlightItems?: Set<string>;
    /** The state before the pending AI changes, drawn dashed. */
    ghost?: Ghost | null;
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
    /** An item or the selected part is dragged by (dx, dy); `end` on release. */
    ondrag?: (target: PlanTarget, dx: number, dy: number, end: boolean) => void;
    /** Right click (or Ctrl+click) without dragging: open the context menu. */
    oncontext?: (target: PlanTarget, client: { x: number; y: number }, world: Point2) => void;
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

  export function fit() {
    const pts = project.parts.flatMap((p) => p.edges.map((e) => e.start)).concat(drawing ?? []);
    if (pts.length === 0) return;
    const xs = pts.map((p) => p.x);
    const ys = pts.map((p) => p.y);
    const w = Math.max(...xs) - Math.min(...xs) || 1;
    const h = Math.max(...ys) - Math.min(...ys) || 1;
    const s = fitScale(width, height, w, h);
    if (s === null) return;
    userView = false;
    scale = s;
    center = { x: (Math.max(...xs) + Math.min(...xs)) / 2, y: (Math.max(...ys) + Math.min(...ys)) / 2 };
  }

  $effect(() => {
    if (!fitted && width > 10) {
      if (project.parts.length) fit();
      else {
        scale = 30;
        center = { x: 8, y: 5 };
      }
      fitted = true;
    }
  });

  // Window resized (e.g. snapped to half the screen) or a bar below the plan grew:
  // keep the whole roof in view until the user zooms or pans.
  $effect(() => {
    void width;
    void height;
    if (untrack(() => fitted && !userView && project.parts.length > 0)) untrack(fit);
  });

  const snapPoint = (px: number, py: number, alt: boolean) => {
    const snap = alt ? 0.001 : SNAP;
    return { x: +(Math.round(wx(px) / snap) * snap).toFixed(3), y: +(Math.round(wy(py) / snap) * snap).toFixed(3) };
  };

  // Pitch and area of every calculated face, written at its centre.
  const tags = $derived(result ? result.parts.flatMap((rp) => faceTags(rp.geometry.faces)) : []);
  const pitchText = (v: number) =>
    `${new Intl.NumberFormat(numberLocale(), { maximumFractionDigits: 2 }).format(v)}°`;
  const areaText = (v: number) =>
    `${new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(v)} m²`;
  /** Rough width of a label in px (monospace, 11 px). */
  const textPx = (s: string) => s.length * 6.8 + 6;

  const sx = (x: number) => (x - center.x) * scale + width / 2;
  const sy = (y: number) => -(y - center.y) * scale + height / 2;
  const wx = (px: number) => (px - width / 2) / scale + center.x;
  const wy = (py: number) => -(py - height / 2) / scale + center.y;

  /** Spacing of the grid lines drawn at this zoom, m. */
  const gridStep = () => (scale * GRID < 8 ? GRID * 10 : scale * GRID < 16 ? GRID * 2 : GRID);

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

  // Dragging.
  let drag: { part: number; vertex: number; sx: number; sy: number; moved: boolean } | null = null;
  let pan: { x: number; y: number; cx: number; cy: number } | null = null;
  /** An item or a whole part being dragged: where the press started. */
  let objDrag: { target: PlanTarget; sx: number; sy: number; wx: number; wy: number; moved: boolean; dx: number; dy: number } | null = null;
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

  /** Captures the pointer for a drag. A synthetic or already released pointer
   * (no active pointer with that id) cannot be captured; the drag then just
   * ends at the next pointerup the plan sees. */
  function capture(e: PointerEvent) {
    try {
      svg!.setPointerCapture(e.pointerId);
    } catch {
      // Not capturable: nothing to do.
    }
  }

  function startObjDrag(e: PointerEvent, target: PlanTarget) {
    const p = localPoint(e);
    const w = snapPoint(p.x, p.y, e.altKey);
    objDrag = { target, sx: p.x, sy: p.y, wx: w.x, wy: w.y, moved: false, dx: 0, dy: 0 };
    capture(e);
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
    startObjDrag(e, { type: "item", item });
  }

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

  function localPoint(e: PointerEvent | WheelEvent) {
    const r = svg!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  function onpointerdown(e: PointerEvent) {
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
      const p = localPoint(e);
      const first = drawing[0];
      // Clicking the first point closes the footprint.
      if (first && drawing.length >= 3 && Math.hypot(sx(first.x) - p.x, sy(first.y) - p.y) < 10) {
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
          ondrag?.(objDrag.target, dx, dy, false);
        }
      }
    } else if (pan) {
      center = { x: pan.cx - (p.x - pan.x) / scale, y: pan.cy + (p.y - pan.y) / scale };
      if (Math.hypot(p.x - pan.x, p.y - pan.y) > 3) userView = true;
    }
  }

  function onpointerup(e: PointerEvent) {
    if (drag?.moved) onmoveend(drag.part, drag.vertex);
    if (!drawing && !placing) guides = { x: null, y: null };
    if (objDrag?.moved) ondrag?.(objDrag.target, objDrag.dx, objDrag.dy, true);
    if (rc) {
      const p = localPoint(e);
      if (Math.hypot(p.x - rc.x, p.y - rc.y) <= DRAG_SLOP) {
        oncontext?.(rc.target, { x: rc.cx, y: rc.cy }, snapPoint(rc.x, rc.y, false));
      }
    }
    drag = null;
    objDrag = null;
    pan = null;
    rc = null;
  }

  function onwheel(e: WheelEvent) {
    e.preventDefault();
    const p = localPoint(e);
    const before = { x: wx(p.x), y: wy(p.y) };
    scale = Math.min(400, Math.max(4, scale * Math.exp(-e.deltaY * 0.0015)));
    userView = true;
    center = { x: center.x + before.x - wx(p.x), y: center.y + before.y - wy(p.y) };
  }

  function startVertexDrag(e: PointerEvent, part: number, vertex: number) {
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
    const p = localPoint(e);
    drag = { part, vertex, sx: p.x, sy: p.y, moved: false };
    capture(e);
  }

  function edgeDblClick(e: MouseEvent, part: number, edge: number) {
    const r = svg!.getBoundingClientRect();
    const x = Math.round(wx(e.clientX - r.left) / SNAP) * SNAP;
    const y = Math.round(wy(e.clientY - r.top) / SNAP) * SNAP;
    oninsert(part, edge, +x.toFixed(3), +y.toFixed(3));
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
    const es = project.parts[part].edges;
    const a = es[i].start;
    const b = es[(i + 1) % es.length].start;
    return Math.hypot(b.x - a.x, b.y - a.y);
  }

  /** Screen position of a wall's dimension label: beside the wall, on the
   * outside of the part, so the wall line never runs through the text. */
  function dimPos(part: number, i: number) {
    const es = project.parts[part].edges;
    const a = es[i].start;
    const b = es[(i + 1) % es.length].start;
    // Shoelace sign: > 0 means counter-clockwise (y up), outside is right of travel.
    let area2 = 0;
    for (let k = 0; k < es.length; k++) {
      const p = es[k].start;
      const q = es[(k + 1) % es.length].start;
      area2 += p.x * q.y - q.x * p.y;
    }
    const len = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    const side = area2 >= 0 ? 1 : -1;
    // Outward normal in world space, then flipped to screen space (y down).
    const nx = (side * (b.y - a.y)) / len;
    const ny = (-side * -(b.x - a.x)) / len;
    const label = `${i + 1}  ${edgeLength(part, i).toFixed(2)}`;
    const halfW = label.length * 3.3;
    const off = 9 + Math.abs(nx) * halfW + Math.abs(ny) * 4;
    return { x: (sx(a.x) + sx(b.x)) / 2 + nx * off, y: (sy(a.y) + sy(b.y)) / 2 + ny * off };
  }
</script>

<div class="wrap" bind:clientWidth={width} bind:clientHeight={height}>
  <svg
    bind:this={svg}
    {width}
    {height}
    role="application"
    aria-label={t("editor.plan")}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
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
        aria-label={part.name}
        onpointerdown={(ev) => pressPart(ev, pi)}
      />
    {/each}

    {#if result}
      {#each result.parts as rp}
        {#each rp.geometry.lines as l}
          {#if l.kind !== "eave"}
            <line
              x1={sx(l.a[0])}
              y1={sy(l.a[1])}
              x2={sx(l.b[0])}
              y2={sy(l.b[1])}
              stroke={lineColor[l.kind]}
              class="roofline"
            />
          {/if}
        {/each}
      {/each}
    {/if}

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
      {#each tags as tag}
        {@const pitch = pitchText(tag.pitch_deg)}
        {@const area = areaText(tag.area)}
        {@const along = tag.along * scale}
        {@const across = tag.across * scale}
        {#if along > textPx(pitch) && across > 14}
          <g class="face-tag" transform="translate({sx(tag.x)} {sy(tag.y)}) rotate({-tag.angle_deg})">
            {#if along > textPx(area) && across > 34}
              <text class="pitch" y="-7">{pitch}</text>
              <text class="area" y="7">{area}</text>
            {:else}
              <text class="pitch">{pitch}</text>
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
          <line class="hit" x1={sx(e.start.x)} y1={sy(e.start.y)} x2={sx(b.x)} y2={sy(b.y)}
            role="button" tabindex="-1" aria-label={t("plan.edge", { n: i + 1 })}
            onpointerdown={(ev) => {
              if (placing || drawing) return;
              selection = { part: pi, edge: i }; dormerSel = null; openSel = null; manualSel = null;
              if (button(ev) === 2) rcTarget = { type: "edge", part: pi, edge: i };
              else ev.stopPropagation();
            }}
            ondblclick={(ev) => edgeDblClick(ev, pi, i)} />
          <line class="wall" x1={sx(e.start.x)} y1={sy(e.start.y)} x2={sx(b.x)} y2={sy(b.y)} />
          {#if showDims && scale * edgeLength(pi, i) > 46}
            {@const lp = dimPos(pi, i)}
            <text x={lp.x} y={lp.y} class="dim edge-dim"><tspan class="no">{i + 1}</tspan>  {edgeLength(pi, i).toFixed(2)}</text>
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
          onpointerdown={(ev) => startVertexDrag(ev, pi, i)}
        />
      {/each}
    {/each}
    {#each project.dormers as d, di}
      {@const fs = d.edges.find((x) => x.direction_rad == null)?.start ?? d.edges[0]?.start}
      {#if fs}
        <g class="dormer" class:sel={dormerSel === di} class:changed={highlightItems.has(`dormer:${di}`)}>
          {#each d.edges as x, k}
            {#if x.direction_rad != null}
              {@const s0 = k === 0 ? -0.8 : 0.8}
              <line class="cheek" x1={sx(x.start.x)} y1={sy(x.start.y)}
                x2={sx(x.start.x + Math.cos(x.direction_rad) * s0)} y2={sy(x.start.y + Math.sin(x.direction_rad) * s0)} />
            {/if}
          {/each}
          <line class="hit" x1={sx(fs.x)} y1={sy(fs.y)} x2={sx(d.end.x)} y2={sy(d.end.y)}
            role="button" tabindex="-1" aria-label={t("plan.dormer", { n: di + 1 })}
            onpointerdown={(ev) => pressItem(ev, { kind: "dormer", index: di })} />
          <line class="front" x1={sx(fs.x)} y1={sy(fs.y)} x2={sx(d.end.x)} y2={sy(d.end.y)} />
        </g>
      {/if}
    {/each}
    {#each project.chimneys as c, ci}
      <path class="opening chimney" class:sel={openSel?.kind === "chimney" && openSel.index === ci} class:changed={highlightItems.has(`chimney:${ci}`)}
        d={"M" + c.corners.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
        role="button" tabindex="-1" aria-label={t("opening.chimneyN", { n: ci + 1 })}
        onpointerdown={(ev) => pressItem(ev, { kind: "chimney", index: ci })} />
    {/each}
    {#each project.roof_openings ?? [] as c, hi}
      <path class="opening hole" class:sel={openSel?.kind === "hole" && openSel.index === hi} class:changed={highlightItems.has(`roof_opening:${hi}`)}
        d={"M" + c.corners.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
        role="button" tabindex="-1" aria-label={t("opening.holeN", { n: hi + 1 })}
        onpointerdown={(ev) => pressItem(ev, { kind: "hole", index: hi })} />
    {/each}
    {#each project.windows ?? [] as w, wi}
      <path class="opening window" class:sel={openSel?.kind === "window" && openSel.index === wi} class:changed={highlightItems.has(`window:${wi}`)}
        d={"M" + windowOutline(result, w).map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
        role="button" tabindex="-1" aria-label={t("opening.windowN", { n: wi + 1 })}
        onpointerdown={(ev) => pressItem(ev, { kind: "window", index: wi })} />
    {/each}
    {#each project.manual_faces ?? [] as f, fi}
      <path class="manual-face" class:sel={manualSel?.kind === "face" && manualSel.index === fi} class:changed={highlightItems.has(`manual_face:${fi}`)}
        d={"M" + f.points.map((p) => `${sx(p.x)},${sy(p.y)}`).join("L") + "Z"}
        role="button" tabindex="-1" aria-label={t("manual.faceN", { n: fi + 1 })}
        onpointerdown={(ev) => pressItem(ev, { kind: "face", index: fi })} />
    {/each}
    {#each project.manual_lines ?? [] as l, li}
      {@const sel = manualSel?.kind === "line" && manualSel.index === li}
      <g class="manual-line" class:sel class:changed={highlightItems.has(`manual_line:${li}`)}>
        <line class="hit" x1={sx(l.a[0])} y1={sy(l.a[1])} x2={sx(l.b[0])} y2={sy(l.b[1])}
          role="button" tabindex="-1" aria-label={t("manual.lineN", { n: li + 1 })}
          onpointerdown={(ev) => pressItem(ev, { kind: "line", index: li })} />
        <line class="stroke" class:measure={!codeKind[l.code]} x1={sx(l.a[0])} y1={sy(l.a[1])} x2={sx(l.b[0])} y2={sy(l.b[1])}
          stroke={codeKind[l.code] ? lineColor[codeKind[l.code]] : undefined} />
        {#if showDims}
          <text class="dim" x={(sx(l.a[0]) + sx(l.b[0])) / 2} y={(sy(l.a[1]) + sy(l.b[1])) / 2} dy="-6">{l.label ? `${l.label} ` : ""}{lineLength(l).toFixed(2)}</text>
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
        {#each drawing as p, i}
          <circle class="draft-pt" class:first={i === 0} cx={sx(p.x)} cy={sy(p.y)} r={i === 0 && drawing.length >= 3 ? 7 : 4} />
        {/each}
        {#if cursorWorld}
          {@const last = drawing[drawing.length - 1]}
          {@const d = Math.hypot(cursorWorld.x - last.x, cursorWorld.y - last.y)}
          {#if d > 0.01}
            <text class="dim draft-dim" x={(sx(last.x) + sx(cursorWorld.x)) / 2} y={(sy(last.y) + sy(cursorWorld.y)) / 2} dy="-8">{d.toFixed(2)}</text>
          {/if}
        {/if}
      {/if}
      {#if cursorWorld}
        <path class="crosshair" d="M{sx(cursorWorld.x) - 9},{sy(cursorWorld.y)}h18M{sx(cursorWorld.x)},{sy(cursorWorld.y) - 9}v18" />
      {/if}
    {/if}
  </svg>
  {#if placing}<div class="hint">{t(placing === "dormer" ? "plan.placeDormer" : placing === "chimney" ? "plan.placeChimney" : placing === "hole" ? "plan.placeHole" : placing === "line" ? (manualDraft.length ? "plan.placeLineEnd" : "plan.placeLine") : placing === "face" ? "plan.placeFace" : "plan.placeWindow")}</div>{:else if !drawing}<div class="hint">{t("plan.hint")}</div>{/if}
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
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
    stroke-linejoin: round;
  }
  .face-tag .pitch {
    fill: var(--text);
    font-size: 12px;
    font-weight: 700;
  }
  .face-tag .area {
    fill: var(--text-2);
    font-size: 11px;
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
  .manual-face {
    fill: color-mix(in srgb, var(--accent) 10%, transparent);
    stroke: var(--accent);
    stroke-width: 1.2;
    stroke-dasharray: 2 3;
    cursor: pointer;
  }
  .manual-face.sel {
    stroke-width: 2.5;
    stroke-dasharray: none;
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
  .roofline {
    stroke-width: 1.8;
    stroke-linecap: round;
  }
  .edge .wall {
    stroke: var(--line-eave);
    stroke-width: 3;
    stroke-linecap: round;
    pointer-events: none;
  }
  .edge.gable .wall {
    stroke: var(--line-verge);
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
  .dim .no {
    font-weight: 700;
    fill: var(--text);
  }
  .edge.sel .wall {
    stroke: var(--focus);
    stroke-width: 5;
    stroke-dasharray: none;
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
