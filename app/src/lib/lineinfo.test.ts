/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";
import { detailOf, lineFigures, lineTitle, sameLine } from "./lineinfo";
import { materialToken } from "./materials";
import { DICTS, i18n } from "./i18n.svelte";
import type { CalcResult, LineDetail, Project } from "./types";

const line = (part: number, index: number, code = "F"): LineDetail => ({
  part,
  index,
  kind: "ridge",
  code,
  a: [0, 0, 6],
  b: [4, 0, 6],
  length: 4,
  counted_length: 4,
  plan_length: 4,
  slope_deg: 0,
  z_low: 6,
  z_high: 6,
  faces: [],
});

describe("line details", () => {
  it("names every line code with a singular name in every language", () => {
    for (const loc of Object.keys(DICTS) as (keyof typeof DICTS)[]) {
      i18n.locale = loc;
      for (const c of ["T", "F", "G", "K", "O", "OL", "OR", "OS", "P", "c", "S", "W", "WV", "WS", "WH"]) {
        const name = lineTitle(c);
        expect(name, `${loc} ${c}`).not.toMatch(/^lineinfo\.|^line\.|^bd\./);
        expect(name, `${loc} ${c}`).not.toBe(DICTS[loc]["line.other"]);
      }
      expect(lineTitle("zz")).toBe(DICTS[loc]["line.other"]);
    }
    i18n.locale = "cs";
    expect(lineTitle("F")).toBe("Hřeben");
    expect(lineTitle("T")).toBe("Okap");
  });

  it("finds a line's details by part and index", () => {
    const result = { line_details: [line(0, 0), line(0, 1, "G"), line(1, 0, "T")] } as unknown as CalcResult;
    expect(detailOf(result, { part: 0, index: 1 })?.code).toBe("G");
    expect(detailOf(result, { part: 1, index: 0 })?.code).toBe("T");
    expect(detailOf(result, { part: 2, index: 0 })).toBeNull();
    expect(detailOf(null, { part: 0, index: 0 })).toBeNull();
    expect(detailOf(result, null)).toBeNull();
    // A result without details (older bridge) shows nothing rather than failing.
    expect(detailOf({} as CalcResult, { part: 0, index: 0 })).toBeNull();
    expect(sameLine({ part: 0, index: 1 }, { part: 0, index: 1 })).toBe(true);
    expect(sameLine({ part: 0, index: 1 }, { part: 1, index: 1 })).toBe(false);
    expect(sameLine(null, { part: 0, index: 1 })).toBe(false);
  });
});

describe("line card figures", () => {
  const project = { parts: [{ edges: [{}, {}, {}, {}] }], material_names: [] } as unknown as Project;

  it("shows the counted length and the drawn one when a chimney cuts the line", () => {
    i18n.locale = "en";
    const d = { ...line(0, 0, "T"), kind: "eave", length: 13, counted_length: 12, plan_length: 13 } as LineDetail;
    const f = lineFigures(d, project);
    expect(f.length).toBe(12);
    expect(f.drawn).toBe(13);
    expect(f.planLength).toBeNull();
    const whole = lineFigures({ ...d, counted_length: 13 }, project);
    expect(whole.length).toBe(13);
    expect(whole.drawn).toBeNull();
  });

  it("names each face's wall, a dormer face as a dormer, and its material and colour", () => {
    i18n.locale = "en";
    const d = {
      ...line(0, 0, "K"),
      kind: "valley",
      slope_deg: 25,
      plan_length: 3,
      length: 3.3,
      counted_length: 3.3,
      faces: [
        { part: 0, face: 0, wall: 2, pitch_deg: 40, material: 2 },
        { part: 0, face: 5, pitch_deg: 45, material: 1 },
      ],
    } as LineDetail;
    const f = lineFigures(d, project);
    expect(f.level).toBe(false);
    expect(f.planLength).toBe(3);
    expect(f.faces.map((x) => x.label)).toEqual(["Wall 3", DICTS.en["lineinfo.dormer"]]);
    expect(f.faces.map((x) => x.token)).toEqual(["--mat-sheet", "--scene-roof"]);
    expect(f.faces[0].material).toBe(DICTS.en["material.2"]);
  });
});

// Vitest stubs CSS imports (even `?raw`), so read the file itself, as
// theme.test.ts does (the UI has no Node type definitions).
const fs: { readFileSync(p: URL, enc: "utf8"): string } = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const css = fs.readFileSync(new URL("../app.css", import.meta.url), "utf8");

describe("material colours", () => {
  it("maps each material to its own token", () => {
    expect([1, 2, 3, 4, 5, 6].map(materialToken)).toEqual([
      "--scene-roof",
      "--mat-sheet",
      "--mat-stone",
      "--mat-slate",
      "--mat-solar",
      "--scene-roof",
    ]);
  });

  it("gives every material its own colour, apart from the tiles', in light and in dark", () => {
    const blocks = css.split(/(?=@media|:root\[data-theme)/);
    const themes = blocks.filter((b: string) => /--mat-sheet:/.test(b));
    expect(themes.length).toBeGreaterThanOrEqual(2);
    for (const b of themes) {
      const v = (name: string) => b.match(new RegExp(`${name}:\\s*([^;]+);`))?.[1].trim();
      const colours = ["--scene-roof", "--mat-sheet", "--mat-stone", "--mat-slate", "--mat-solar"].map(v);
      expect(colours.every(Boolean), b.slice(0, 60)).toBe(true);
      expect(new Set(colours).size).toBe(colours.length);
    }
  });
});
