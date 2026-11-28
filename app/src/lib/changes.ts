// Human-readable descriptions of project changes (see `tectora_core::diff`).

import { m, m2, num } from "./format";
import { numberLocale, t } from "./i18n.svelte";
import { partLabel } from "./partname";
import { materialName } from "./materials";
import { windowOutline } from "./openings";
import { lineName } from "./linecodes";
import { roughFootprint } from "./towers";
import type { CalcResult, Change, Chimney, Dormer, FigureDifference, HalfHip, ManualFace, ManualLine, Point2, Project, RoofWindow, Tower } from "./types";

const isVertical = (p: number) => Math.abs(p - 90) <= 0.001;
const deg = (v: unknown) => `${num(Number(v)).replace(/[.,]00$/, "")}°`;
const len = (v: unknown) => `${num(Number(v))} m`;

/** Fields of item changes (with `item`) and the label of one such item. */
const ITEM_LABEL: Record<string, string> = {
  dormer: "plan.dormer",
  chimney: "opening.chimneyN",
  window: "opening.windowN",
  roof_opening: "opening.holeN",
  manual_line: "manual.lineN",
  manual_face: "manual.faceN",
  tower: "tower.n",
};

/** True for a change of one dormer, chimney, window, opening or hand-drawn line or face. */
export const isItemChange = (c: Change) =>
  isChange(c) && Number.isInteger(c.item) && Object.prototype.hasOwnProperty.call(ITEM_LABEL, c.field);

/** Whether `c` has the shape of a change at all: a received change log can hold anything (HUNT-301). */
const isChange = (c: unknown): c is Change =>
  typeof c === "object" && c !== null && typeof (c as Change).field === "string";

const index = (v: unknown): number | undefined => (Number.isInteger(v) && (v as number) >= 0 ? (v as number) : undefined);
const isPt = (v: unknown): v is Point2 =>
  typeof v === "object" && v !== null && Number.isFinite((v as Point2).x) && Number.isFinite((v as Point2).y);
const isPts = (v: unknown): v is Point2[] => Array.isArray(v) && v.every(isPt);
const isXy = (v: unknown): v is number[] => Array.isArray(v) && v.length >= 2 && Number.isFinite(v[0]) && Number.isFinite(v[1]);
const count = (v: unknown) => (Array.isArray(v) ? v.length : 0);

/** Most characters of a text shown in a change row. */
const MAX_SHOWN = 120;

/** A text value of a change as one line of plain text: control, bidi and
    other invisible characters removed, cut to {@link MAX_SHOWN} (HUNT-301). */
export function shownText(v: unknown): string {
  const s = typeof v === "string" ? v : v == null ? "" : typeof v === "number" || typeof v === "boolean" ? String(v) : "";
  const kept = s.replace(/[\p{Cc}\p{Cf}\u2028\u2029]/gu, "");
  const chars = Array.from(kept);
  return chars.length > MAX_SHOWN ? chars.slice(0, MAX_SHOWN - 1).join("") + "…" : kept;
}

/** Most field differences listed under one changed tower. */
const MAX_TOWER_DETAILS = 6;

/** A tower's length as the dialog writes it: 2 decimals, a third when it has one (0,375 m) (UX-005). */
const towerLen = (v: unknown) => `${new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 3 }).format(Number(v))} m`;

type TierLike = Partial<Tower["tiers"][number]>;
const sameTier = (x: unknown, y: unknown) => JSON.stringify(x) === JSON.stringify(y);

/** One tier's values in a line ("Převýšení 1,20 m · Šířka nahoře 0,00 m · Břidlice"), for an added or removed tier. */
function tierValues(x: TierLike, project: Project | null): string {
  const v: string[] = [];
  const fin = (n: unknown) => typeof n === "number" && Number.isFinite(n);
  if (fin(x.rise)) v.push(`${t("tower.rise")} ${towerLen(x.rise)}`);
  if (fin(x.top_width)) v.push(`${t("tower.topWidth")} ${towerLen(x.top_width)}`);
  if (fin(x.bulge) && x.bulge !== 0) v.push(`${t("tower.bulge")} ${towerLen(x.bulge)}`);
  v.push(materialName(project, Number(x.material ?? 2)));
  return v.join(" · ");
}

/**
 * The tiers of two versions matched before they are compared: the tiers
 * both start and end with are left out, so one tier added or removed reads
 * as that, not as every tier above it changed (UX-005). What is left is
 * compared pairwise; the rest of the longer side is added or removed.
 * Pairs `[before index | null, after index | null]`.
 */
export function matchTiers(ta: unknown[], tb: unknown[]): [number | null, number | null][] {
  let p = 0;
  while (p < ta.length && p < tb.length && sameTier(ta[p], tb[p])) p++;
  let s = 0;
  while (s < ta.length - p && s < tb.length - p && sameTier(ta[ta.length - 1 - s], tb[tb.length - 1 - s])) s++;
  const ma = ta.length - p - s;
  const mb = tb.length - p - s;
  const out: [number | null, number | null][] = [];
  for (let k = 0; k < Math.max(ma, mb); k++) out.push([k < ma ? p + k : null, k < mb ? p + k : null]);
  return out;
}

/** A tower's name in a change row: its label, else its number. */
function towerName(c: Change): string {
  const pick = (v: unknown) => (v && typeof v === "object" ? shownText((v as Tower).label) : "");
  return pick(c.after) || pick(c.before) || t("tower.n", { n: (c.item ?? 0) + 1 });
}

/**
 * What an outside save changed in a tower, field by field ("Pás 3:
 * Převýšení 0,9 m → 1,4 m"), for the expanded review list (audit UX-008).
 * Empty for any other change, or for values of the wrong type.
 */
export function towerDetails(c: Change, project: Project | null): string[] {
  if (!isChange(c) || c.field !== "tower" || c.kind !== "modified") return [];
  const a = c.before as Partial<Tower> | null;
  const b = c.after as Partial<Tower> | null;
  if (!a || !b || typeof a !== "object" || typeof b !== "object") return [];
  const out: string[] = [];
  const fin = (v: unknown) => typeof v === "number" && Number.isFinite(v);
  const diff = (label: string, x: unknown, y: unknown, fmt: (v: unknown) => string) => {
    if (x === y || (x == null && y == null)) return;
    if (x != null && !fin(x) && typeof x !== "string") return;
    if (y != null && !fin(y) && typeof y !== "string") return;
    out.push(`${label}: ${x == null ? "—" : fmt(x)} → ${y == null ? "—" : fmt(y)}`);
  };
  const shape = (v: unknown) => (v === "rectangle" || v === "polygon" || v === "circle" ? t(`tower.shape.${v}`) : "—");
  diff(t("tower.label"), a.label ?? "", b.label ?? "", (v) => shownText(v) || "—");
  diff(t("tower.shape"), a.shape ?? "rectangle", b.shape ?? "rectangle", shape);
  diff(t("tower.width"), a.width, b.width, towerLen);
  diff(t("tower.depth"), a.depth, b.depth, towerLen);
  diff(t("tower.sides"), a.sides, b.sides, (v) => num(Number(v)));
  diff(t("tower.cornerRadius"), a.corner_radius ?? 0, b.corner_radius ?? 0, towerLen);
  diff(t("tower.rotation"), a.rotation_deg ?? 0, b.rotation_deg ?? 0, deg);
  diff(t("tower.baseHeight"), a.base_height, b.base_height, towerLen);
  diff(t("edge.overhang"), a.overhang ?? 0, b.overhang ?? 0, towerLen);
  if (isPt(a.center) && isPt(b.center) && (a.center.x !== b.center.x || a.center.y !== b.center.y)) {
    out.push(t("chg.towerMoved", { ax: num(a.center.x), ay: num(a.center.y), x: num(b.center.x), y: num(b.center.y) }));
  }
  const ta = Array.isArray(a.tiers) ? a.tiers : [];
  const tb = Array.isArray(b.tiers) ? b.tiers : [];
  for (const [i, j] of matchTiers(ta, tb)) {
    const x = i === null ? null : ta[i];
    const y = j === null ? null : tb[j];
    if (x && typeof x === "object" && !y) {
      out.push(t("chg.tierRemoved", { n: i! + 1, v: tierValues(x, project) }));
      continue;
    }
    if (y && typeof y === "object" && !x) {
      out.push(t("chg.tierAdded", { n: j! + 1, v: tierValues(y, project) }));
      continue;
    }
    if (!x || !y || typeof x !== "object" || typeof y !== "object") continue;
    const tier = t("tower.tierN", { n: j! + 1 });
    diff(`${tier}, ${t("tower.rise")}`, x.rise, y.rise, towerLen);
    diff(`${tier}, ${t("tower.topWidth")}`, x.top_width, y.top_width, towerLen);
    diff(`${tier}, ${t("tower.bulge")}`, x.bulge ?? 0, y.bulge ?? 0, towerLen);
    diff(`${tier}, ${t("edge.material")}`, x.material ?? 2, y.material ?? 2, (v) => materialName(project, Number(v)));
  }
  return out.length > MAX_TOWER_DETAILS ? [...out.slice(0, MAX_TOWER_DETAILS - 1), t("review.more", { n: out.length - MAX_TOWER_DETAILS + 1 })] : out;
}

export function describeChange(c: Change, project: Project | null): string {
  if (!isChange(c)) return t("chg.other");
  try {
    return describeValid(c, project);
  } catch {
    // A value of the wrong type from a received log: never break the list.
    return t("chg.other");
  }
}

function describeValid(c: Change, project: Project | null): string {
  const multiPart = (project?.parts.length ?? 0) > 1;
  const n = (index(c.edge) ?? 0) + 1;
  const part = index(c.part);
  const prefix = multiPart && part !== undefined ? t("chg.part", { p: part + 1 }) + " · " : "";
  const text = (() => {
    if (isItemChange(c)) {
      // A tower by its name, as the plan and Results show it (UX-005).
      const name = c.field === "tower" ? towerName(c) : t(ITEM_LABEL[c.field], { n: c.item! + 1 });
      return t(c.kind === "added" ? "chg.itemAdded" : c.kind === "removed" ? "chg.itemRemoved" : "chg.itemModified", { name });
    }
    switch (c.field) {
      case "pitch_deg": {
        const a = Number(c.before);
        const b = Number(c.after);
        if (isVertical(b)) return t("chg.toGable", { n });
        if (isVertical(a)) return t("chg.toEave", { n, b: deg(b) });
        return t("chg.pitch", { n, a: deg(a), b: deg(b) });
      }
      case "eave_height":
        return t("chg.eaveHeight", { n, a: len(c.before), b: len(c.after) });
      case "overhang":
        return t("chg.overhang", { n, a: len(c.before), b: len(c.after) });
      case "half_hip": {
        const b = c.after as HalfHip | null;
        if (!b || typeof b !== "object") return t("chg.halfHipOff", { n });
        return t("chg.halfHip", { n, h: len(b.height), p: deg(b.pitch_deg) });
      }
      case "priority":
        return t(Number(c.after) > 0 ? "chg.priorityOn" : "chg.priorityOff", { n });
      case "windows":
        return t("chg.windows", { k: count(c.after) });
      case "chimneys":
        return t("chg.chimneys", { k: count(c.after) });
      case "breaks":
        return t("chg.breaks", { n });
      case "clad":
        return t(c.after ? "chg.cladOn" : "chg.cladOff", { n });
      case "direction":
        return c.after == null ? t("chg.directionOff", { n }) : t("chg.direction", { n, b: deg(((Number(c.after) * 180) / Math.PI + 360) % 360) });
      case "dormers":
        return t("chg.dormers", { k: count(c.after) });
      case "roof_openings":
        return t("chg.holes", { k: count(c.after) });
      case "material":
        return t("chg.material", { n, b: materialName(project, Number(c.after)) });
      case "material_names":
        return t("chg.materialNames");
      case "flat_roof":
        return t(c.after ? "chg.flatOn" : "chg.flatOff");
      case "result_snapshot":
        return t(c.kind === "removed" ? "chg.snapshotDropped" : "chg.snapshotStored");
      case "vertex": {
        if (!isXy(c.after)) return t("chg.other");
        const [x, y] = c.after;
        return t("chg.vertex", { n, x: num(x), y: num(y) });
      }
      case "edge":
        return c.kind === "added" ? t("chg.vertexAdded", { n }) : t("chg.vertexRemoved", { n });
      case "part":
        return c.kind === "added"
          ? t("chg.partAdded", { name: partLabel(shownText(c.after), part ?? 0) })
          : t("chg.partRemoved", { name: partLabel(shownText(c.before), part ?? 0) });
      case "building_height":
        return t("chg.buildingHeight", { a: len(c.before), b: len(c.after) });
      case "part_name":
        return t("chg.partName", { b: partLabel(shownText(c.after), part ?? 0) });
      case "name":
        return t("chg.name", { b: shownText(c.after) });
      case "notes":
        return t("chg.notes");
      case "job.status":
        // Before and after, as every other row (UX-207); an added status has no before.
        if (!STATUSES.has(String(c.after)) || (c.before && !STATUSES.has(String(c.before)))) return t("chg.other");
        return c.before
          ? t("chg.jobStatus", { field: t("job.status"), a: t(`status.${c.before}`), b: t(`status.${c.after}`) })
          : t("chg.job", { field: t("job.status"), b: t(`status.${c.after}`) });
      default:
        if (c.field.startsWith("job.")) {
          return JOB_FIELDS.has(c.field) ? t("chg.job", { field: t(c.field), b: shownText(c.after) || "—" }) : t("chg.other");
        }
        return t("chg.other");
    }
  })();
  return prefix + text;
}

/** Corners added or removed in one building from which the list says only how many (an arc, UX-013). */
const MANY_CORNERS = 3;

/**
 * The rows of a list of changes: as `describeChange`, but the corners one
 * save added to and removed from one building (a rounded corner, an apse:
 * eight "new corner" rows) are one row with their counts (UX-013).
 */
export function describeChanges(changes: Change[], project: Project | null): string[] {
  const valid = changes.filter(isChange);
  const corners = new Map<number, { added: number; removed: number }>();
  for (const c of valid) {
    const p = index(c.part);
    if (c.field !== "edge" || p === undefined) continue;
    const k = corners.get(p) ?? { added: 0, removed: 0 };
    if (c.kind === "added") k.added++;
    else if (c.kind === "removed") k.removed++;
    corners.set(p, k);
  }
  const many = (p: number | undefined) => p !== undefined && (corners.get(p)?.added ?? 0) + (corners.get(p)?.removed ?? 0) >= MANY_CORNERS;
  const multiPart = (project?.parts.length ?? 0) > 1;
  const out: string[] = [];
  const said = new Set<number>();
  for (const c of changes) {
    const p = isChange(c) ? index(c.part) : undefined;
    if (isChange(c) && c.field === "edge" && many(p)) {
      if (said.has(p!)) continue;
      said.add(p!);
      const k = corners.get(p!)!;
      const prefix = multiPart ? t("chg.part", { p: p! + 1 }) + " · " : "";
      out.push(prefix + t("chg.cornersMany", { a: k.added, r: k.removed }));
      continue;
    }
    out.push(describeChange(c, project));
  }
  return out;
}

const STATUSES = new Set(["draft", "offer", "ordered", "done"]);
const JOB_FIELDS = new Set(["job.number", "job.customer", "job.address"]);

/** Edges touched by a set of changes, as "part:edge" keys for highlighting. */
export function changedEdges(changes: Change[], project: Project): Set<string> {
  const out = new Set<string>();
  for (const c of changes) {
    if (!isChange(c)) continue;
    const part = index(c.part);
    const edge = index(c.edge);
    if (c.kind === "removed" || part === undefined) continue;
    const n = project.parts[part]?.edges.length ?? 0;
    if (edge === undefined || n === 0) {
      if (c.field === "part") for (let i = 0; i < n; i++) out.add(`${part}:${i}`);
      continue;
    }
    out.add(`${part}:${edge}`);
    // A moved or inserted vertex also changes the edge that ends there.
    if (c.field === "vertex" || c.field === "edge") out.add(`${part}:${(edge + n - 1) % n}`);
  }
  return out;
}

/** Dormers, chimneys, … touched by a set of changes, as "field:item" keys
    into the current project (removed items are not there any more). */
export function changedItems(changes: Change[]): Set<string> {
  changes = changes.filter(isChange);
  const out = new Set<string>();
  for (const c of changes) if (isItemChange(c) && c.kind !== "removed") out.add(`${c.field}:${c.item}`);
  return out;
}

/** The outlines of what a set of changes replaced or removed, taken from the
    version before them, so the plan can show the previous state dashed. */
export interface Ghost {
  /** Closed outlines: walls of changed parts, chimneys, openings, windows, faces. */
  shapes: Point2[][];
  /** Open lines: dormer fronts, hand-drawn lines. */
  lines: [Point2, Point2][];
}

const samePts = (a: Point2[], b: Point2[]) =>
  a.length === b.length && a.every((p, i) => Math.abs(p.x - b[i].x) < 1e-6 && Math.abs(p.y - b[i].y) < 1e-6);

/** Changes that reshape the roof without moving an outline: the previous roof lines show them (UX-017). */
const ROOF_FIELDS = new Set([
  "pitch_deg",
  "eave_height",
  "overhang",
  "half_hip",
  "priority",
  "breaks",
  "clad",
  "direction",
  "building_height",
  "flat_roof",
  "vertex",
  "edge",
  "part",
]);

/** The plan lines of a calculated roof, as the core gave them. */
const planLines = (r: CalcResult | null): [Point2, Point2][] =>
  (r?.parts ?? []).flatMap((p) => (p.geometry.lines ?? []).map((l) => [{ x: l.a[0], y: l.a[1] }, { x: l.b[0], y: l.b[1] }] as [Point2, Point2]));

const sameSeg = (a: [Point2, Point2], b: [Point2, Point2]) =>
  samePts(a, b) || samePts(a, [b[1], b[0]]);

/**
 * `beforeResult` is the core's roof of `before`: for a roof-definition change
 * its lines that are not in the current roof are part of the ghost, so a
 * changed pitch or eave shows where the roof was.
 */
export function ghostOf(
  changes: Change[],
  before: Project | null,
  now: Project,
  result: CalcResult | null,
  beforeResult: CalcResult | null = null,
): Ghost {
  const ghost: Ghost = { shapes: [], lines: [] };
  if (!before) return ghost;
  changes = changes.filter(isChange);
  if (beforeResult && changes.some((c) => ROOF_FIELDS.has(c.field))) {
    const nowLines = planLines(result);
    for (const l of planLines(beforeResult)) {
      if (Math.hypot(l[1].x - l[0].x, l[1].y - l[0].y) < 1e-6) continue;
      if (!nowLines.some((n) => sameSeg(n, l))) ghost.lines.push(l);
    }
  }
  // Walls: every part of the previous version whose outline is not there
  // any more, when the changes moved, added or removed corners or parts.
  const outline = (p: Project["parts"][number]) => p.edges.map((e) => e.start);
  const nowOutlines = now.parts.map(outline);
  const geometry = changes.some((c) => c.field === "vertex" || c.field === "edge" || c.field === "part");
  for (const p of geometry ? before.parts : []) {
    const o = outline(p);
    if (o.length >= 2 && !nowOutlines.some((n) => samePts(n, o))) ghost.shapes.push(o);
  }
  for (const c of changes) {
    if (!isItemChange(c) || c.kind === "added" || !c.before || typeof c.before !== "object") continue;
    // A received log's item may have any shape: only well-formed ones are drawn (HUNT-301).
    try {
      switch (c.field) {
        case "chimney":
        case "roof_opening": {
          const corners = (c.before as Chimney).corners;
          if (isPts(corners)) ghost.shapes.push(corners);
          break;
        }
        case "window": {
          const o = windowOutline(result, c.before as RoofWindow);
          if (isPts(o)) ghost.shapes.push(o);
          break;
        }
        case "manual_face": {
          const pts = (c.before as ManualFace).points;
          if (isPts(pts)) ghost.shapes.push(pts);
          break;
        }
        case "tower": {
          const o = roughFootprint(c.before as Tower);
          if (isPts(o)) ghost.shapes.push(o);
          break;
        }
        case "manual_line": {
          const l = c.before as ManualLine;
          if (isXy(l.a) && isXy(l.b)) ghost.lines.push([{ x: l.a[0], y: l.a[1] }, { x: l.b[0], y: l.b[1] }]);
          break;
        }
        case "dormer": {
          const d = c.before as Dormer;
          if (!Array.isArray(d.edges)) break;
          const fs = d.edges.find((x) => x?.direction_rad == null)?.start ?? d.edges[0]?.start;
          if (isPt(fs) && isPt(d.end)) ghost.lines.push([fs, d.end]);
          break;
        }
      }
    } catch {
      // Not drawn.
    }
  }
  return ghost;
}

/** Label of a figure key (`roof_area`, `ridge_height`, `line:<code>`, `material:<n>`). */
export function figureLabel(key: string, project: Project | null): string {
  if (key === "roof_area") return t("res.area");
  if (key === "ridge_height") return t("snap.ridge");
  if (key.startsWith("line:")) return key.slice(5);
  return materialName(project, Number(key.slice(9)));
}

/** A figure's name in the stored-offer comparison: a line with its code and name ("OL Verge left", UX-205). */
export function figureName(key: string, project: Project | null): string {
  return key.startsWith("line:") ? `${key.slice(5)} ${lineName(key.slice(5))}` : figureLabel(key, project);
}

export function formatFigure(key: string, v: number): string {
  return key === "ridge_height" || key.startsWith("line:") ? m(v) : m2(v);
}

/** "Roof area 182.40 → 195.12 m² (+12.72)". */
export function describeFigure(d: FigureDifference, project: Project | null): string {
  const delta = d.current - d.stored;
  return t("chg.figure", {
    label: figureLabel(d.key, project),
    a: num(d.stored),
    b: formatFigure(d.key, d.current),
    d: `${delta > 0 ? "+" : "−"}${num(Math.abs(delta))}`,
  });
}
