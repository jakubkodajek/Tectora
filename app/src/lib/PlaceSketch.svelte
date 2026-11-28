<script lang="ts">
  // A small drawing of the roof face a chimney, roof window or opening sits
  // on, turned so its eave is at the bottom: left on the drawing is then the
  // "left edge" of the distances (looking up from the eave). It shows the
  // three distances of `Placement` as the plan's dimension lines do, so the
  // object dialog explains them while it covers the plan. Presentation only:
  // the numbers come from the core (edit::position).
  import { numberLocale, t } from "./i18n.svelte";
  import type { CalcResult, Placement, Point2 } from "./types";
  import type { OpeningKind } from "./openings";
  import type { PlaceKey } from "./PlaceFields.svelte";
  import { maxOf, minOf } from "./extent";

  let {
    result,
    placement,
    outline,
    kind,
    active = null,
  }: {
    result: CalcResult | null;
    placement: Placement | null;
    /** The object's plan outline. */
    outline: Point2[];
    kind: OpeningKind;
    /** The distance whose field has the focus: drawn solid, the others dimmed. */
    active?: PlaceKey | null;
  } = $props();

  const W = 300;
  /** The drawing's height follows the face's shape, between these. */
  const H_MIN = 130;
  const H_MAX = 300;
  const PAD = { l: 34, r: 34, t: 30, b: 32 };

  const mm = (v: number) =>
    new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 3, maximumFractionDigits: 3, useGrouping: false }).format(v);

  const unit = (a: Point2, b: Point2): Point2 | null => {
    const dx = b.x - a.x, dy = b.y - a.y;
    const l = Math.hypot(dx, dy);
    return l > 1e-3 && Number.isFinite(l) ? { x: dx / l, y: dy / l } : null;
  };

  const drawing = $derived.by(() => {
    const pl = placement;
    if (!pl || !result) return null;
    const faces = (result.parts[pl.part]?.geometry.faces ?? []).filter((f) => f.edge === pl.edge && f.polygon.length >= 3);
    if (!faces.length) return null;
    // `eave_line` runs from the eave up the slope; the edges' lines run along the eave to the right.
    let up = unit(pl.eave_line[0], pl.eave_line[1]);
    if (!up) {
      const right = unit(pl.left_line[0], pl.left_line[1]) ?? unit(pl.right_line[0], pl.right_line[1]) ?? { x: 1, y: 0 };
      up = { x: -right.y, y: right.x };
    }
    const right = { x: up.y, y: -up.x };
    const u = up;
    const turn = (p: Point2) => ({ x: p.x * right.x + p.y * right.y, y: -(p.x * u.x + p.y * u.y) });
    const facePts = faces.map((f) => f.polygon.map(([x, y]) => turn({ x, y })));
    const obj = outline.filter((p) => Number.isFinite(p.x) && Number.isFinite(p.y)).map(turn);
    const all = [...facePts.flat(), ...obj];
    const x0 = minOf(all, (p) => p.x), x1 = maxOf(all, (p) => p.x);
    const y0 = minOf(all, (p) => p.y), y1 = maxOf(all, (p) => p.y);
    const k = Math.min((W - PAD.l - PAD.r) / Math.max(x1 - x0, 1e-3), (H_MAX - PAD.t - PAD.b) / Math.max(y1 - y0, 1e-3));
    if (!Number.isFinite(k) || k <= 0) return null;
    const H = Math.max(H_MIN, Math.min(H_MAX, PAD.t + PAD.b + (y1 - y0) * k));
    const ox = PAD.l + (W - PAD.l - PAD.r - (x1 - x0) * k) / 2;
    const oy = PAD.t + (H - PAD.t - PAD.b - (y1 - y0) * k) / 2;
    const s = (p: Point2) => ({ x: ox + (p.x - x0) * k, y: oy + (p.y - y0) * k });
    const toScreen = (p: Point2) => s(turn(p));
    const path = (pts: Point2[]) => pts.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");
    // The eave: the edges of a face at its lowest height.
    const eaves: [Point2, Point2][] = [];
    for (const f of faces) {
      const zmin = minOf(f.polygon, (q) => q[2]);
      f.polygon.forEach((a, i) => {
        const b = f.polygon[(i + 1) % f.polygon.length];
        if (Math.abs(a[2] - zmin) < 1e-3 && Math.abs(b[2] - zmin) < 1e-3) eaves.push([toScreen({ x: a[0], y: a[1] }), toScreen({ x: b[0], y: b[1] })]);
      });
    }
    const lowest = eaves.length ? maxOf(eaves, ([a, b]) => Math.max(a.y, b.y)) : oy + (y1 - y0) * k;
    const eaveMid = eaves.length
      ? (() => {
          const [a, b] = eaves.reduce((m, e) => (Math.hypot(e[1].x - e[0].x, e[1].y - e[0].y) > Math.hypot(m[1].x - m[0].x, m[1].y - m[0].y) ? e : m));
          return (a.x + b.x) / 2;
        })()
      : W / 2;
    const dim = (key: PlaceKey, line: [Point2, Point2], v: number) => {
      const a = toScreen(line[0]), b = toScreen(line[1]);
      const len = Math.hypot(b.x - a.x, b.y - a.y);
      const dir = len > 1e-6 ? { x: (b.x - a.x) / len, y: (b.y - a.y) / len } : { x: 1, y: 0 };
      const n = { x: -dir.y * 4, y: dir.x * 4 };
      const ticks = [a, b].map((p) => `M${(p.x - n.x).toFixed(1)},${(p.y - n.y).toFixed(1)}L${(p.x + n.x).toFixed(1)},${(p.y + n.y).toFixed(1)}`).join("");
      let lx: number, ly: number, anchor: "start" | "middle" | "end";
      if (key === "eave") {
        lx = (a.x + b.x) / 2 + 6;
        ly = (a.y + b.y) / 2 + 4;
        anchor = "start";
      } else if (len >= 44) {
        lx = (a.x + b.x) / 2;
        ly = (a.y + b.y) / 2 - 7;
        anchor = "middle";
      } else if (key === "left") {
        lx = Math.min(a.x, b.x) - 4;
        ly = (a.y + b.y) / 2 - 7;
        anchor = "end";
      } else {
        lx = Math.max(a.x, b.x) + 4;
        ly = (a.y + b.y) / 2 - 7;
        anchor = "start";
      }
      return { key, a, b, ticks, text: mm(v), lx, ly, anchor };
    };
    return {
      h: H,
      faces: facePts.map((pts) => path(pts.map(s))),
      eaves,
      eaveLabel: { x: eaveMid, y: Math.min(lowest + 18, H - 6) },
      object: obj.length >= 3 ? path(obj.map(s)) : null,
      dims: [dim("eave", pl.eave_line, pl.from_eave), dim("left", pl.left_line, pl.from_left), dim("right", pl.right_line, pl.from_right)],
    };
  });

  const alt = $derived(
    placement ? t("place.sketchAlt", { e: mm(placement.from_eave), l: mm(placement.from_left), r: mm(placement.from_right) }) : t("place.offRoof"),
  );
</script>

<figure class="sketch" data-testid="place-sketch">
  <figcaption>{t("place.sketch")}</figcaption>
  {#if drawing}
    <svg viewBox="0 0 {W} {drawing.h}" role="img" aria-label={alt}>
      {#each drawing.faces as f}<polygon class="face" points={f} />{/each}
      {#each drawing.eaves as [a, b]}<line class="eave" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />{/each}
      <text class="eave-mark" x={drawing.eaveLabel.x} y={drawing.eaveLabel.y}>{t("place.eaveMark")}</text>
      {#if drawing.object}<polygon class="obj {kind}" points={drawing.object} />{/if}
      {#each drawing.dims as d (d.key)}
        <g class="dim" class:on={active === d.key} class:off={active !== null && active !== d.key} data-dim={d.key}>
          <line x1={d.a.x} y1={d.a.y} x2={d.b.x} y2={d.b.y} />
          <path d={d.ticks} />
          <text x={d.lx} y={d.ly} text-anchor={d.anchor}>{d.text}</text>
        </g>
      {/each}
    </svg>
  {:else}
    <p class="none">{t("place.offRoof")}</p>
  {/if}
</figure>

<style>
  .sketch {
    position: relative;
    margin: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    min-height: 120px;
  }
  figcaption {
    position: absolute;
    left: 8px;
    top: 6px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  svg {
    display: block;
    width: 100%;
    height: auto;
  }
  .face {
    fill: var(--plan-fill);
    stroke: var(--text-3);
    stroke-width: 1;
    stroke-linejoin: round;
  }
  .eave {
    stroke: var(--line-eave);
    stroke-width: 2.5;
    stroke-linecap: round;
  }
  .eave-mark {
    font-size: 11px;
    fill: var(--line-eave);
    text-anchor: middle;
  }
  .obj {
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linejoin: round;
    fill: color-mix(in srgb, var(--text-3) 35%, transparent);
  }
  .obj.hole {
    fill: var(--surface);
    stroke-dasharray: 4 3;
  }
  .obj.window {
    fill: color-mix(in srgb, var(--window) 28%, transparent);
  }
  .dim line,
  .dim path {
    stroke: var(--accent);
    stroke-width: 1.2;
    fill: none;
  }
  .dim line {
    stroke-dasharray: 5 3;
  }
  .dim text {
    font-size: 12px;
    font-weight: 600;
    fill: var(--accent);
    font-variant-numeric: tabular-nums;
    paint-order: stroke;
    stroke: var(--bg);
    stroke-width: 3px;
  }
  .dim.on line {
    stroke-dasharray: none;
    stroke-width: 2;
  }
  .dim.off {
    opacity: 0.5;
  }
  .none {
    margin: 0;
    padding: 34px 16px 16px;
    font-size: 12.5px;
    color: var(--text-2);
    text-align: center;
  }
</style>
