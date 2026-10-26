// Human-readable descriptions of project changes (see `tectora_core::diff`).

import { num } from "./format";
import { t } from "./i18n.svelte";
import type { Change, HalfHip, Project } from "./types";

const GABLE = 89.999;
const deg = (v: unknown) => `${num(Number(v)).replace(/[.,]00$/, "")}°`;
const len = (v: unknown) => `${num(Number(v))} m`;

export function describeChange(c: Change, multiPart: boolean): string {
  const n = (c.edge ?? 0) + 1;
  const prefix = multiPart && c.part !== undefined ? t("chg.part", { p: c.part + 1 }) + " · " : "";
  const text = (() => {
    switch (c.field) {
      case "pitch_deg": {
        const a = Number(c.before);
        const b = Number(c.after);
        if (b >= GABLE) return t("chg.toGable", { n });
        if (a >= GABLE) return t("chg.toEave", { n, b: deg(b) });
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
      case "vertex": {
        const [x, y] = c.after as [number, number];
        return t("chg.vertex", { n, x: num(x), y: num(y) });
      }
      case "edge":
        return c.kind === "added" ? t("chg.vertexAdded", { n }) : t("chg.vertexRemoved", { n });
      case "part":
        return c.kind === "added"
          ? t("chg.partAdded", { name: String(c.after) })
          : t("chg.partRemoved", { name: String(c.before) });
      case "part_name":
        return t("chg.partName", { b: String(c.after) });
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
