import { describe, expect, it } from "vitest";
import { chooseLabels, insidePolygon, overlaps, placeDimSpots, placeLabels, rotatedBox, segCrossesBox, segIndex, underHigherRoof, wallNumber, type Box, type Seg } from "./planlabels";

describe("rotatedBox", () => {
  it("is the label box itself without a turn and swaps sides at 90°", () => {
    expect(rotatedBox(10, 20, 40, 10, 0)).toEqual({ x0: -10, y0: 15, x1: 30, y1: 25 });
    const b = rotatedBox(0, 0, 40, 10, 90);
    expect(b.x1 - b.x0).toBeCloseTo(10, 9);
    expect(b.y1 - b.y0).toBeCloseTo(40, 9);
  });
});

describe("placeLabels", () => {
  it("keeps labels that do not overlap and drops a later one that does (UX-012)", () => {
    const dim = { x0: 0, y0: 0, x1: 40, y1: 12 };
    const tagOverDim = { x0: 20, y0: 5, x1: 70, y1: 30 };
    const tagApart = { x0: 100, y0: 0, x1: 150, y1: 30 };
    expect(placeLabels([dim, tagOverDim, tagApart])).toEqual([true, false, true]);
  });

  it("never keeps two overlapping labels and skips labels not drawn", () => {
    const boxes = Array.from({ length: 30 }, (_, i) => rotatedBox((i % 6) * 25, Math.floor(i / 6) * 9, 40, 14, i * 17));
    const keep = placeLabels([null, ...boxes]);
    expect(keep[0]).toBe(false);
    const kept = boxes.filter((_, i) => keep[i + 1]);
    for (let i = 0; i < kept.length; i++) for (let j = i + 1; j < kept.length; j++) expect(overlaps(kept[i], kept[j])).toBe(false);
  });

  it("lets labels that only touch stay", () => {
    expect(placeLabels([{ x0: 0, y0: 0, x1: 10, y1: 10 }, { x0: 10, y0: 0, x1: 20, y1: 10 }])).toEqual([true, true]);
  });
});

describe("underHigherRoof", () => {
  it("is true only when another roof lies clearly above the face", () => {
    expect(underHigherRoof(3.2, 5)).toBe(true);
    expect(underHigherRoof(3.2, 3.22)).toBe(false);
    expect(underHigherRoof(3.2, null)).toBe(false);
    expect(underHigherRoof(null, 5)).toBe(false);
  });
});

describe("chooseLabels (UX-018)", () => {
  it("takes a later candidate when the first overlaps a label kept before", () => {
    const a = { x0: 0, y0: 0, x1: 40, y1: 12 };
    const outside = { x0: 30, y0: 2, x1: 70, y1: 14 };
    const inside = { x0: 80, y0: 2, x1: 120, y1: 14 };
    expect(chooseLabels([[a], [outside, inside]])).toEqual([0, 1]);
  });
  it("skips unusable candidates and drops a label with none that fits", () => {
    const a = { x0: 0, y0: 0, x1: 40, y1: 12 };
    expect(chooseLabels([[a], [null, { x0: 50, y0: 0, x1: 60, y1: 10 }], [{ ...a }, null], []])).toEqual([0, 1, -1, -1]);
  });
});

describe("insidePolygon", () => {
  const sq = [
    { x: 0, y: 0 },
    { x: 4, y: 0 },
    { x: 4, y: 4 },
    { x: 0, y: 4 },
  ];
  it("tells points inside a building from points beside it", () => {
    expect(insidePolygon({ x: 2, y: 2 }, sq)).toBe(true);
    expect(insidePolygon({ x: 4.3, y: 2 }, sq)).toBe(false);
    expect(insidePolygon({ x: -1, y: -1 }, sq)).toBe(false);
  });
});

describe("wallNumber (UX-018)", () => {
  it("prefixes the building only when there are several", () => {
    expect(wallNumber(0, 2, 1)).toBe("3");
    expect(wallNumber(1, 2, 2)).toBe("2.3");
    expect(wallNumber(0, 0, 2)).toBe("1.1");
  });
});

describe("labels of several buildings keep off roof lines and walls (sweep 2026-11-15)", () => {
  const box = { x0: 0, y0: 0, x1: 60, y1: 16 };
  it("segCrossesBox: a line through the label, not one beside it or grazing its edge", () => {
    expect(segCrossesBox([-10, 8, 70, 8], box)).toBe(true);
    expect(segCrossesBox([30, -20, 30, 40], box)).toBe(true);
    expect(segCrossesBox([30, 2, 30, 12], box)).toBe(true);
    expect(segCrossesBox([-10, 20, 70, 20], box)).toBe(false);
    expect(segCrossesBox([-10, 1, 70, 1], box)).toBe(false);
    expect(segCrossesBox([62, -5, 62, 30], box)).toBe(false);
    expect(segCrossesBox([-20, -20, -5, 30], box)).toBe(false);
  });
  it("a shared wall's length moves off the ridge to a free spot along the wall", () => {
    // Outside is on the other building (null); inside at the middle is on
    // the ridge (y = 108); inside off the middle is free.
    const ridge: Seg = [0, 108, 300, 108];
    const inMid = { x0: 200, y0: 100, x1: 260, y1: 116 };
    const inOff = { x0: 200, y0: 60, x1: 260, y1: 76 };
    expect(chooseLabels([[null, inMid, null, inOff]], () => [ridge])).toEqual([3]);
    // Without a free spot it stays at the middle, on the line.
    expect(chooseLabels([[null, inMid]], () => [ridge])).toEqual([1]);
    // On lines everywhere: the spot on the fewest.
    const hip: Seg = [230, 90, 250, 125];
    expect(chooseLabels([[null, inMid, null, inOff]], () => [ridge, hip, [0, 70, 300, 70]])).toEqual([3]);
    expect(chooseLabels([[inMid, inOff]], () => [ridge, [0, 70, 300, 70], [0, 65, 300, 65]])).toEqual([0]);
    // A label over a kept one is never taken to dodge a line.
    expect(chooseLabels([[inOff], [null, inMid, null, { ...inOff }]], () => [ridge])).toEqual([0, 1]);
  });
  it("a face label that would cross a wall shrinks to the pitch and keeps off the wall's length", () => {
    const wall: Seg = [450, 300, 450, 500];
    const dim = { x0: 455, y0: 392, x1: 520, y1: 408 };
    const full = { x0: 416, y0: 380, x1: 457, y1: 420 };
    const pitch = { x0: 426, y0: 388, x1: 448, y1: 412 };
    const lines = (i: number) => (i === 0 ? [] : [wall]);
    expect(chooseLabels([[dim], [full, pitch]], lines)).toEqual([0, 1]);
    const wide = { x0: 380, y0: 380, x1: 470, y1: 420 };
    expect(chooseLabels([[dim], [wide]], lines)).toEqual([0, -1]);
  });
});

describe("placement dimension labels (UI-209)", () => {
  it("offers another spot for a dimension label, and a later label avoids a kept one", () => {
    const a = placeDimSpots({ mx: 100, my: 100, x1: 80, y1: 100, x2: 120, y2: 100 }, "2,381", false);
    const b = placeDimSpots({ mx: 104, my: 100, x1: 104, y1: 80, x2: 104, y2: 120 }, "3,700", false);
    expect(a.length).toBeGreaterThan(1);
    expect(overlaps(a[0].box, b[0].box)).toBe(true);
    const chosen = chooseLabels([a.map((s) => s.box), b.map((s) => s.box)]);
    expect(chosen[0]).toBe(0);
    expect(chosen[1]).toBeGreaterThan(0);
    expect(overlaps(a[chosen[0]].box, b[chosen[1]].box)).toBe(false);
  });
});

// UX-602: a plan of 200 buildings tested every label against every wall,
// every roof line and every label kept before, at each zoom and pan step.
describe("segIndex and the label pass at scale", () => {
  /** A fixed pseudo-random sequence in 0..1. */
  const seq = (seed: number) => () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  };

  it("finds exactly the segments whose box meets the asked box, each once, in order", () => {
    const r = seq(7);
    const segs: Seg[] = Array.from({ length: 600 }, () => {
      const [x, y] = [r() * 3000 - 500, r() * 2000 - 500];
      // Short and very long ones, also vertical and horizontal.
      const len = r() < 0.2 ? 2500 : 120;
      return [x, y, x + (r() - 0.5) * len * (r() < 0.1 ? 0 : 1), y + (r() - 0.5) * len * (r() < 0.1 ? 0 : 1)];
    });
    const index = segIndex(segs);
    expect(index.size).toBe(600);
    for (let k = 0; k < 300; k++) {
      const [x, y] = [r() * 2600 - 300, r() * 1800 - 300];
      const b: Box = { x0: x, y0: y, x1: x + 20 + r() * 200, y1: y + 10 + r() * 60 };
      const want = segs.flatMap((s, i) => (Math.max(s[0], s[2]) >= b.x0 && Math.min(s[0], s[2]) <= b.x1 && Math.max(s[1], s[3]) >= b.y0 && Math.min(s[1], s[3]) <= b.y1 ? [i] : []));
      expect(index.nearAt(b)).toEqual(want);
      expect(index.near(b)).toEqual(want.map((i) => segs[i]));
      // Every segment that crosses the box is among them.
      segs.forEach((s, i) => {
        if (segCrossesBox(s, b)) expect(want).toContain(i);
      });
    }
  });

  it("keeps only the segments that meet the window, and still finds every crossing inside it", () => {
    const r = seq(11);
    const segs: Seg[] = Array.from({ length: 400 }, () => {
      const [x, y] = [r() * 6000 - 2500, r() * 4000 - 1500];
      return [x, y, x + (r() - 0.5) * 3000, y + (r() - 0.5) * 3000];
    });
    const view: Box = { x0: -160, y0: -160, x1: 1160, y1: 860 };
    const index = segIndex(segs, view);
    expect(index.size).toBeLessThan(400);
    for (let k = 0; k < 200; k++) {
      const [x, y] = [r() * 900, r() * 700];
      const b: Box = { x0: x, y0: y, x1: x + 90, y1: y + 30 };
      const near = index.nearAt(b);
      segs.forEach((s, i) => {
        if (segCrossesBox(s, b)) expect(near).toContain(i);
      });
    }
  });

  it("chooses the same candidates with indexed lines as with the plain list", () => {
    const r = seq(3);
    const lines: Seg[] = Array.from({ length: 300 }, () => {
      const [x, y] = [r() * 1400, r() * 900];
      return [x, y, x + (r() - 0.5) * 300, y + (r() - 0.5) * 300];
    });
    const cands = Array.from({ length: 250 }, () => {
      const [x, y] = [r() * 1300, r() * 850];
      return Array.from({ length: 4 }, (_, c) => (r() < 0.15 ? null : rotatedBox(x + c * 14, y + c * 9, 50 + r() * 40, 16, r() * 90)));
    });
    const index = segIndex(lines);
    expect(chooseLabels(cands, () => index)).toEqual(chooseLabels(cands, () => lines));
    // And as the unindexed rule says: no two kept labels overlap.
    const chosen = chooseLabels(cands, () => index);
    const kept = chosen.flatMap((j, i) => (j >= 0 ? [cands[i][j]!] : []));
    for (let i = 0; i < kept.length; i++) for (let j = i + 1; j < kept.length; j++) expect(overlaps(kept[i], kept[j])).toBe(false);
  });

  it("compares a label only with what is near it: 200 buildings cost a few tests per label, not every wall and label", () => {
    // 200 buildings of 4 walls in a grid, 140 px apart; a wall length on each wall.
    const walls: Seg[] = [];
    const cands: (Box | null)[][] = [];
    for (let i = 0; i < 200; i++) {
      const [ox, oy] = [(i % 20) * 140, Math.floor(i / 20) * 120];
      const c = [[ox, oy], [ox + 100, oy], [ox + 100, oy + 80], [ox, oy + 80]];
      c.forEach((p, k) => {
        const q = c[(k + 1) % 4];
        walls.push([p[0], p[1], q[0], q[1]]);
        const [mx, my] = [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
        cands.push([rotatedBox(mx, my - 14, 44, 16, 0), rotatedBox(mx, my + 14, 44, 16, 0)]);
      });
    }
    const stats = { overlaps: 0, lines: 0 };
    const index = segIndex(walls);
    const chosen = chooseLabels(cands, () => index, stats);
    expect(chosen.filter((j) => j >= 0).length).toBeGreaterThan(700);
    // 800 labels x 2 candidates: against every wall that would be 1.28 million line tests.
    expect(stats.lines).toBeLessThan(800 * 2 * 8);
    expect(stats.overlaps).toBeLessThan(800 * 2 * 12);
  });
});
