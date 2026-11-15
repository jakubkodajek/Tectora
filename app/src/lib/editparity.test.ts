// The editor's TypeScript operations against tectora_core::edit (TEST-010,
// ARCH-006): `crates/tectora-core/tests/edit_parity.rs` writes inputs with the
// core's outputs to tests/fixtures/edit/parity.json; here the same inputs go
// through the TypeScript copies that are left (roundroof.ts, transform.ts and
// dormers.ts; ARCH-004 replaces them by core calls module by module, see
// docs/adr). A difference means the UI and an AI client (MCP) would get
// different roofs from the same dialog: fix the TypeScript copy to match the
// core. The fixture keeps the cases of copies that are gone: the core's own
// test reads them (`edit_parity.rs`), only this file has nothing left to
// compare them with.
import { describe, expect, it } from "vitest";
import * as rr from "./roundroof";
import * as tf from "./transform";
import * as dm from "./dormers";
import { roundTo } from "./rounding";

const files = import.meta.glob(["../../../tests/fixtures/edit/parity.json"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const data = JSON.parse(Object.values(files)[0] ?? "{}") as { outlines: Record<string, any[]>; project: any; cases: { op: string; in: any; out: any }[] };

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));

/** Replaces the file's references to its shared outlines and project. */
function resolve(v: any): any {
  if (Array.isArray(v)) return v.map(resolve);
  if (v && typeof v === "object") {
    if (v.$ref === "outline") return clone(data.outlines[v.name]);
    if (v.$ref === "edge") return clone(data.outlines[v.name][v.i]);
    if (v.$ref === "project") return clone(data.project);
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, resolve(x)]));
  }
  return v;
}

/** Drops what the core's file format leaves out (defaults, nulls, empty lists). */
function norm(v: any): any {
  if (Array.isArray(v)) return v.map(norm);
  if (v instanceof Map) return norm(Object.fromEntries([...v].map(([k, x]) => [String(k), x])));
  if (v && typeof v === "object") {
    const out: Record<string, any> = {};
    for (const [k, x] of Object.entries(v)) {
      if (x == null || x === false || x === "" || (Array.isArray(x) && x.length === 0)) continue;
      if ((k === "material" && x === 1) || (k === "priority" && x === 0)) continue;
      out[k] = norm(x);
    }
    return out;
  }
  return v;
}

/** The first path where `a` and `b` differ (numbers within 1e-9), or null. */
function diff(a: any, b: any, path = ""): string | null {
  if (typeof a === "number" && typeof b === "number") {
    return Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(a), Math.abs(b)) ? null : `${path}: ${a} vs ${b}`;
  }
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return `${path}: length ${a.length} vs ${b.length}`;
    for (let i = 0; i < a.length; i++) {
      const d = diff(a[i], b[i], `${path}[${i}]`);
      if (d) return d;
    }
    return null;
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
      const d = diff(a[k], b[k], `${path}.${k}`);
      if (d) return d;
    }
    return null;
  }
  return a === b ? null : `${path}: ${JSON.stringify(a)} vs ${JSON.stringify(b)}`;
}

function planMap(m: any): tf.PlanMap {
  if (m.kind === "move") return tf.translation(m.dx, m.dy);
  if (m.kind === "rotate") return tf.rotation(m.centre, m.deg);
  return tf.mirror(m.centre, m.deg);
}

/** Operations whose TypeScript copy was deleted (ARCH-004): the editor calls the core. */
const GONE = new Set([
  "triangle", "triangle_patch", "build_up", "solve", "shift_for", "roof_height", "same_way", "inward", "move_wall",
  "eave_below_gable", "merge_onto", "merge_by_moving", "hip_pitches", "hip_pitches_from_eaves", "meeting", "parallel",
  "opposite_wall", "span_between", "part_centre", "offset_wall", "regular_polygon",
]);

/** Runs one case through the TypeScript copy; the result in the core's JSON shape. */
function run(op: string, a: any): any {
  switch (op) {
    case "arc_for":
      return rr.arcFor(a);
    case "segments_for":
      return rr.segmentsFor(a.deg, a.radius);
    case "transform_part": {
      const parts = tf.transformPart(a.project, a.pi, planMap(a.map), a.copies);
      return { project: a.project, parts };
    }
    case "plan_map":
      return planMap(a.map).apply(a.p);
    case "inside_outline":
      return tf.insideOutline(a.part, a.p);
    case "default_params":
      return dm.defaultParams(a.kind, a.start, a.angle, a.eave);
    case "build_dormer":
      return dm.buildDormer(a);
    case "parse_dormer":
      return dm.parseDormer(a);
    case "nearest_wall_angle":
      return dm.nearestWallAngle(a.parts, a.p);
    case "dormer_section":
      return dm.dormerSection(a.p, a.host);
    case "eave_for": {
      const v = dm.eaveFor(a.p, a.host, a.target);
      return typeof v === "number" ? v : { error: true };
    }
    case "solve_dormer": {
      const v = dm.solveDormer(a.p, a.host, a.target, a.width);
      return typeof v === "string" ? { error: true } : v;
    }
    case "pitch_for_back_width": {
      const v = dm.pitchForBackWidth(a.p, a.host, a.width);
      return typeof v === "number" ? v : { error: true };
    }
  }
  throw new Error(`no TypeScript operation for ${op}: add it to editparity.test.ts`);
}

describe("editor operations match tectora_core::edit", () => {
  it("reads the cases the core wrote", () => {
    expect(data.cases?.length ?? 0).toBeGreaterThan(1000);
  });

  // Only the operations that still have a TypeScript copy (`run` knows them).
  const ops = [...new Set((data.cases ?? []).map((c) => c.op))].filter((op) => !GONE.has(op));
  for (const op of ops) {
    it(op, () => {
      const bad: string[] = [];
      data.cases.forEach((c, i) => {
        if (c.op !== op) return;
        const got = norm(run(op, resolve(c.in)));
        const d = diff(got, norm(c.out));
        if (d) bad.push(`case ${i}${d}\n  in ${JSON.stringify(c.in).slice(0, 300)}`);
      });
      expect(bad.slice(0, 10), `${bad.length} of the ${op} cases differ from the core`).toEqual([]);
    });
  }
});

describe("roundTo", () => {
  it("rounds like the core's round_to: halves away from zero, by the binary value times 10^d", () => {
    expect(roundTo(9.9995, 3)).toBe(10); // toFixed(3) gave 9.999
    expect(roundTo(-0.0005, 3)).toBe(-0.001); // Math.round gave 0
    expect(roundTo(2.5, 0)).toBe(3);
    expect(roundTo(-2.5, 0)).toBe(-3);
    expect(roundTo(33.69006752, 4)).toBe(33.6901);
  });
});
