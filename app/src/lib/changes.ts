// Human-readable descriptions of project changes (see `tectora_core::diff`).

import { m, m2, num } from "./format";
import { t } from "./i18n.svelte";
import { partLabel } from "./partname";
import { materialName } from "./materials";
import { windowOutline } from "./openings";
import type { CalcResult, Change, Chimney, Dormer, FigureDifference, HalfHip, ManualFace, ManualLine, Point2, Project, RoofWindow } from "./types";

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
};

/** True for a change of one dormer, chimney, window, opening or hand-drawn line or face. */
export const isItemChange = (c: Change) => c.item !== undefined && c.field in ITEM_LABEL;

export function describeChange(c: Change, project: Project | null): string {
  const multiPart = (project?.parts.length ?? 0) > 1;
  const n = (c.edge ?? 0) + 1;
  const prefix = multiPart && c.part !== undefined ? t("chg.part", { p: c.part + 1 }) + " · " : "";
  const text = (() => {
    if (isItemChange(c)) {
      const name = t(ITEM_LABEL[c.field], { n: c.item! + 1 });
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
        if (!b) return t("chg.halfHipOff", { n });
        return t("chg.halfHip", { n, h: len(b.height), p: deg(b.pitch_deg) });
      }
      case "priority":
        return t(Number(c.after) > 0 ? "chg.priorityOn" : "chg.priorityOff", { n });
      case "windows":
        return t("chg.windows", { k: (c.after as unknown[]).length });
      case "chimneys":
        return t("chg.chimneys", { k: (c.after as unknown[]).length });
      case "breaks":
        return t("chg.breaks", { n });
      case "clad":
        return t(c.after ? "chg.cladOn" : "chg.cladOff", { n });
      case "direction":
        return c.after == null ? t("chg.directionOff", { n }) : t("chg.direction", { n, b: deg(((Number(c.after) * 180) / Math.PI + 360) % 360) });
      case "dormers":
        return t("chg.dormers", { k: (c.after as unknown[]).length });
      case "roof_openings":
        return t("chg.holes", { k: (c.after as unknown[]).length });
      case "material":
        return t("chg.material", { n, b: materialName(project, Number(c.after)) });
      case "material_names":
        return t("chg.materialNames");
      case "flat_roof":
        return t(c.after ? "chg.flatOn" : "chg.flatOff");
      case "result_snapshot":
        return t(c.kind === "removed" ? "chg.snapshotDropped" : "chg.snapshotStored");
      case "vertex": {
        const [x, y] = c.after as [number, number];
        return t("chg.vertex", { n, x: num(x), y: num(y) });
      }
      case "edge":
        return c.kind === "added" ? t("chg.vertexAdded", { n }) : t("chg.vertexRemoved", { n });
      case "part":
        return c.kind === "added"
          ? t("chg.partAdded", { name: partLabel(String(c.after), c.part ?? 0) })
          : t("chg.partRemoved", { name: partLabel(String(c.before), c.part ?? 0) });
      case "building_height":
        return t("chg.buildingHeight", { a: len(c.before), b: len(c.after) });
      case "part_name":
        return t("chg.partName", { b: partLabel(String(c.after), c.part ?? 0) });
      case "name":
        return t("chg.name", { b: String(c.after) });
      case "notes":
        return t("chg.notes");
      case "job.status":
        return t("chg.job", { field: t("job.status"), b: t(`status.${c.after}`) });
      default:
        if (c.field.startsWith("job.")) {
          return t("chg.job", { field: t(c.field), b: String(c.after) || "—" });
        }
        return c.field;
    }
  })();
  return prefix + text;
}

/** Edges touched by a set of changes, as "part:edge" keys for highlighting. */
export function changedEdges(changes: Change[], project: Project): Set<string> {
  const out = new Set<string>();
  for (const c of changes) {
    if (c.kind === "removed" || c.part === undefined) continue;
    const n = project.parts[c.part]?.edges.length ?? 0;
    if (c.edge === undefined || n === 0) {
      if (c.field === "part") for (let i = 0; i < n; i++) out.add(`${c.part}:${i}`);
      continue;
    }
    out.add(`${c.part}:${c.edge}`);
    // A moved or inserted vertex also changes the edge that ends there.
    if (c.field === "vertex" || c.field === "edge") out.add(`${c.part}:${(c.edge + n - 1) % n}`);
  }
  return out;
}

/** Dormers, chimneys, … touched by a set of changes, as "field:item" keys
    into the current project (removed items are not there any more). */
export function changedItems(changes: Change[]): Set<string> {
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

export function ghostOf(changes: Change[], before: Project | null, now: Project, result: CalcResult | null): Ghost {
  const ghost: Ghost = { shapes: [], lines: [] };
  if (!before) return ghost;
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
    if (!isItemChange(c) || c.kind === "added" || !c.before) continue;
    switch (c.field) {
      case "chimney":
      case "roof_opening":
        ghost.shapes.push((c.before as Chimney).corners);
        break;
      case "window":
        ghost.shapes.push(windowOutline(result, c.before as RoofWindow));
        break;
      case "manual_face":
        ghost.shapes.push((c.before as ManualFace).points);
        break;
      case "manual_line": {
        const l = c.before as ManualLine;
        ghost.lines.push([{ x: l.a[0], y: l.a[1] }, { x: l.b[0], y: l.b[1] }]);
        break;
      }
      case "dormer": {
        const d = c.before as Dormer;
        const fs = d.edges.find((x) => x.direction_rad == null)?.start ?? d.edges[0]?.start;
        if (fs) ghost.lines.push([fs, d.end]);
        break;
      }
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
