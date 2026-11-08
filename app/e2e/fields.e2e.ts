// Number fields: typed values must be stored however the user leaves the
// field (Enter, Tab, a click on the plan, on another wall or on a button),
// with a decimal comma or dot, and never on the wrong wall. Regression tests
// for "typed values are not applied, only the spinner arrows work".
import type { Locator, Page } from "@playwright/test";
import { bar, openEditor, planXY, selectWall, setField, tool } from "./editor";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

/** Types into a field like a person: click, select all, type (no fill()). */
async function typeInto(page: Page, field: Locator, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

/** Every number in `v` by its dotted path ("0.edges.1.start.x"). */
function flat(v: unknown, path = "", out: Record<string, unknown> = {}): Record<string, unknown> {
  if (v !== null && typeof v === "object") {
    for (const [k, x] of Object.entries(v as Record<string, unknown>)) flat(x, path ? `${path}.${k}` : k, out);
  } else out[path] = v;
  return out;
}

/** Clicks the plan at world point (x, y) with a real mouse press. */
async function pressPlan(page: Page, x: number, y: number) {
  const p = await planXY(page, x, y);
  await page.mouse.click(p.x, p.y);
}

test.describe("number fields", () => {
  test("Enter stores the typed value, with a decimal comma", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const eave = def.getByLabel(cs("edge.eaveHeight"));
    await typeInto(page, eave, "3,75");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === 3.75);
    // Shown in the UI language (Czech: decimal comma), focus stays.
    await expect(eave).toHaveValue("3,75");
    await expect(eave).toBeFocused();
    await typeInto(page, eave, "4.5");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === 4.5);
  });

  test("a click on the plan stores the value before it changes the selection", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const before = (await loadProject(id)).parts[0].edges.map((e: any) => e.overhang);
    // Type, then click straight on the second wall: the value belongs to wall 1.
    await typeInto(page, def.getByLabel(cs("edge.overhang")), "0,85");
    const w = (await loadProject(id)).parts[0].edges;
    await pressPlan(page, (w[1].start.x + w[2].start.x) / 2, (w[1].start.y + w[2].start.y) / 2);
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 2 }));
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.85);
    expect(p.parts[0].edges.slice(1).map((e: any) => e.overhang)).toEqual(before.slice(1));
    await expect(def.getByLabel(cs("edge.overhang"))).toHaveValue(String(before[1]).replace(".", ","));

    // Type, then click on empty plan: stored too.
    await typeInto(page, def.getByLabel(cs("edge.pitch")), "33");
    await pressPlan(page, -6, -6);
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 33);
  });

  test("a click on a button uses the value just typed", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await typeInto(page, def.getByLabel(cs("edge.eaveHeight")), "5");
    await def.getByRole("button", { name: cs("edge.heightAll") }).click();
    await waitForProject(id, (p) => p.parts[0].edges.every((e: any) => e.eave_height === 5));
    // Next-wall button: stored on the wall it was typed for.
    await typeInto(page, def.getByLabel(cs("edge.pitch")), "31");
    await def.getByRole("button", { name: cs("def.next") }).click();
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 2 }));
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 31);
    expect(p.parts[0].edges[1].pitch_deg).not.toBe(31);
  });

  // macOS WebKit does not move the focus to a clicked button, so the field
  // is still focused (and has not sent `change`) when the button's handler
  // runs. A real click in Chromium blurs first and cannot show that order;
  // these presses keep the focus in the field, as WebKit does (TEST-001).
  test("a press on a button that keeps the focus in the field (WebKit) uses the value just typed", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const press = async (name: string) => {
      const b = def.getByRole("button", { name });
      await b.dispatchEvent("pointerdown", { button: 0 });
      await b.dispatchEvent("click");
    };
    // Next: the typed pitch belongs to wall 1, not to wall 2.
    const pitch = def.getByLabel(cs("edge.pitch"));
    await typeInto(page, pitch, "31");
    await press(cs("def.next"));
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 2 }));
    await page.keyboard.press("Tab");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 31);
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts[0].edges[1].pitch_deg).not.toBe(31);
    // Apply to all: uses the eave height just typed, not the stored one.
    await press(cs("def.prev"));
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 1 }));
    await typeInto(page, def.getByLabel(cs("edge.eaveHeight")), "5,5");
    await press(cs("edge.heightAll"));
    await waitForProject(id, (p) => p.parts[0].edges.every((e: any) => e.eave_height === 5.5));
  });

  test("text typed while an AI save arrives is kept and then stored with the AI change", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const f = def.getByLabel(cs("edge.overhang"));
    await typeInto(page, f, "0,85");
    await mcp.call("update_edges", { id, changes: [{ index: 2, pitch_deg: 20 }], summary: "Wall 3 at 20°." });
    // The app has taken the AI save (the wall is highlighted for review).
    await expect(page.locator(".edge.changed").first()).toBeAttached();
    await page.waitForTimeout(500);
    await expect(f).toHaveValue("0,85");
    await expect(f).toBeFocused();
    await f.press("Tab");
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.85 && p.parts[0].edges[2].pitch_deg === 20);
  });

  test("Escape restores, invalid text is refused and marked, arrows and ▲▼ step", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const pitch = def.getByLabel(cs("edge.pitch"));
    const start = (await loadProject(id)).parts[0].edges[0].pitch_deg as number;

    await typeInto(page, pitch, "12");
    await page.keyboard.press("Escape");
    await expect(pitch).toHaveValue(String(start));
    // Escape in the field does not clear the wall selection.
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 1 }));

    await typeInto(page, pitch, "abc");
    await expect(pitch).toHaveAttribute("aria-invalid", "true");
    await page.keyboard.press("Tab");
    await expect(pitch).toHaveValue(String(start));
    await typeInto(page, pitch, "500");
    await page.keyboard.press("Enter");
    await expect(pitch).toHaveValue(String(start));
    // The refusal says why, then the restored value is plain again.
    await expect(pitch).toHaveAttribute("title", cs("num.range", { min: "0,1", max: "179,9" }));
    await expect(pitch).not.toHaveAttribute("aria-invalid", "true", { timeout: 5000 });
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(start);

    await pitch.focus();
    await page.keyboard.press("ArrowUp");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === start + 1);
    const eave = def.getByLabel(cs("edge.eaveHeight"));
    const h = (await loadProject(id)).parts[0].edges[0].eave_height as number;
    await eave.locator("xpath=following-sibling::span[contains(@class,'num-spin')]//button[1]").click();
    await waitForProject(id, (p) => Math.abs(p.parts[0].edges[0].eave_height - (h + 0.05)) < 1e-9);
    await eave.locator("xpath=following-sibling::span[contains(@class,'num-spin')]//button[2]").click();
    await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === h);
  });

  test("every number field of the roof definition bar takes a typed value", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await def.getByRole("button", { name: cs("edge.addBreak") }).click();
    await waitForProject(id, (p) => (p.parts[0].edges[0].breaks ?? []).length === 1);
    const fields = def.locator("input.num-input:not([disabled])");
    const n = await fields.count();
    expect(n).toBeGreaterThanOrEqual(8);
    // Where each field must land: exactly that value of wall 1 becomes
    // the typed one and nothing else in the roof changes (TEST-011).
    const target: Record<string, string> = {
      [cs("edge.startX")]: "0.edges.0.start.x",
      [cs("edge.startY")]: "0.edges.0.start.y",
      [cs("edge.pitch")]: "0.edges.0.pitch_deg",
      [cs("edge.eaveHeight")]: "0.edges.0.eave_height",
      [cs("edge.overhang")]: "0.edges.0.overhang",
      [cs("edge.breakHeight", { n: 1 })]: "0.edges.0.breaks.0.height",
      [cs("edge.breakPitch")]: "0.edges.0.breaks.0.pitch_deg",
    };
    const seen: string[] = [];
    for (let i = 0; i < n; i++) {
      const before = flat((await loadProject(id)).parts);
      const f = fields.nth(i);
      const label = (await f.evaluate((el) => el.closest("label")?.querySelector("span")?.textContent ?? "")) || `#${i}`;
      const cur = Number((await f.inputValue()).replace(",", "."));
      await typeInto(page, f, String(cur + 0.5).replace(".", ","));
      await page.keyboard.press("Enter");
      await expect.poll(async () => JSON.stringify(flat((await loadProject(id)).parts)) !== JSON.stringify(before), { message: label, timeout: 6000 }).toBe(true);
      const after = flat((await loadProject(id)).parts);
      const changed = Object.keys({ ...before, ...after }).filter((k) => before[k] !== after[k]);
      seen.push(label);
      if (label === cs("edge.length")) {
        // The wall's end corner moves along the wall: only wall 2's start.
        expect(changed.every((k) => k.startsWith("0.edges.1.start.")), `${label}: ${changed}`).toBe(true);
        const e = (await loadProject(id)).parts[0].edges;
        expect(Math.hypot(e[1].start.x - e[0].start.x, e[1].start.y - e[0].start.y)).toBeCloseTo(cur + 0.5, 3);
      } else {
        expect(changed, label).toEqual([target[label]]);
        expect(after[target[label]], label).toBeCloseTo(cur + 0.5, 6);
      }
    }
    expect(seen).toEqual(expect.arrayContaining([cs("edge.length"), ...Object.keys(target)]));
  });

  test("chimney and dormer fields take typed values", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.chimney").click();
    await pressPlan(page, 3, 2);
    const cb = bar(page, "opening.chimney");
    await waitForProject(id, (p) => p.chimneys.length === 1);
    const field = cb.locator("input.num-input").first();
    const before = JSON.stringify((await loadProject(id)).chimneys[0]);
    await typeInto(page, field, "1,25");
    await pressPlan(page, -6, -6);
    await expect.poll(async () => JSON.stringify((await loadProject(id)).chimneys[0]) !== before).toBe(true);

    await tool(page, "tool.dormer").click();
    await pressPlan(page, 4, 1);
    const db = bar(page, "dormer.title");
    await waitForProject(id, (p) => p.dormers.length === 1);
    const dormer = JSON.stringify((await loadProject(id)).dormers[0]);
    await typeInto(page, db.getByLabel(cs("dormer.width")), "2,4");
    await page.keyboard.press("Tab");
    await expect.poll(async () => JSON.stringify((await loadProject(id)).dormers[0]) !== dormer).toBe(true);
    await expect(db.getByLabel(cs("dormer.width"))).toHaveValue("2,4");
  });

  test("drawing presets and settings defaults take a decimal comma", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await page.locator("body").press("Insert");
    const draw = page.getByRole("toolbar", { name: cs("draw.title") });
    await expect(draw).toBeVisible();
    const eave = draw.locator(".presets").getByLabel(cs("edge.eaveHeight"));
    await typeInto(page, eave, "2,75");
    await page.keyboard.press("Enter");
    // Enter in a preset field stores it and does not close the footprint.
    await expect(draw).toBeVisible();
    await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("tectora.presets") ?? "{}").eave_height)).toBe(2.75);
    await setField(eave, "3");
    await page.keyboard.press("Escape");
    expect((await loadProject(id)).parts).toHaveLength(1);
  });

  test("a manual face is labelled and can be turned into a building", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.face").click();
    await pressPlan(page, 9, 1);
    await pressPlan(page, 12.5, 1);
    await pressPlan(page, 12.5, 7);
    await pressPlan(page, 9, 7);
    await pressPlan(page, 9, 1);
    await waitForProject(id, (p) => p.manual_faces?.length === 1);
    await expect(page.locator(".manual-tag")).toHaveText(`${cs("manual.faceN", { n: 1 })} · 30°`);
    await bar(page, "manual.face").getByRole("button", { name: cs("manual.toPart") }).click();
    const p = await waitForProject(id, (p) => p.parts.length === 2 && !p.manual_faces?.length);
    expect(p.parts[1].edges).toHaveLength(4);
    expect(p.parts[1].name).toBe(cs("editor.partN", { n: 2 }));
    await expect(page.locator(".statusbar")).toContainText(cs("status.walls", { n: 8, p: 2 }));
  });

  test("a text label typed and followed by a click on another line stays on its own line", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    for (const y of [2, 6]) {
      await tool(page, "tool.line").click();
      await pressPlan(page, 2, y);
      await pressPlan(page, 8, y);
    }
    await waitForProject(id, (p) => p.manual_lines?.length === 2);
    const mb = bar(page, "manual.line");
    await expect(mb.locator(".which strong")).toHaveText(cs("manual.lineN", { n: 2 }));
    await typeInto(page, mb.getByLabel(cs("manual.label")), "Okap dvůr");
    // Straight onto the first line in the plan.
    await pressPlan(page, 5, 2);
    await expect(mb.locator(".which strong")).toHaveText(cs("manual.lineN", { n: 1 }));
    const p = await waitForProject(id, (p) => p.manual_lines[1].label === "Okap dvůr");
    expect(p.manual_lines[0].label ?? "").toBe("");
  });

  test("after a click on ▲ the arrow keys step the field, not the selected object", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.chimney").click();
    await pressPlan(page, 3, 2);
    await waitForProject(id, (p) => p.chimneys.length === 1);
    const cb = bar(page, "opening.chimney");
    const top = cb.getByLabel(cs("opening.top"));
    const before = (await loadProject(id)).chimneys[0];
    await top.locator("xpath=following-sibling::span[contains(@class,'num-spin')]//button[1]").click();
    await expect(top).toBeFocused();
    await page.keyboard.press("ArrowUp");
    const p = await waitForProject(id, (p) => Math.abs(p.chimneys[0].height - (before.height + 0.1)) < 1e-9);
    expect(p.chimneys[0].corners).toEqual(before.corners);
  });
});

test.describe("number fields: bounds and the footprint bar", () => {
  test("a value beyond what the roof calculation takes is refused and marked; the file keeps the old one", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const before = JSON.stringify((await loadProject(id)).parts);
    // A field without bounds of its own still takes only what the core
    // accepts (±100 000, model::MAX_ABS_VALUE): e.g. 3 m typed in mm.
    for (const key of ["edge.eaveHeight", "edge.startX", "edge.startY", "edge.overhang"]) {
      const f = def.getByLabel(cs(key));
      const shown = await f.inputValue();
      await typeInto(page, f, "1e9");
      await expect(f, key).toHaveAttribute("aria-invalid", "true");
      await page.keyboard.press("Enter");
      await expect(f, key).toHaveValue(shown);
      await expect(f, key).toHaveAttribute("title", /100000/);
    }
    await page.waitForTimeout(700);
    expect(JSON.stringify((await loadProject(id)).parts)).toBe(before);
    await expect(page.locator(".roofline").first()).toBeAttached();
  });

  test("the footprint wall length reads numbers like every field and refuses text", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.draw").click();
    const draw = bar(page, "draw.title");
    await pressPlan(page, 13.5, 1);
    await expect(draw).toContainText(cs("draw.points", { n: 1 }));
    const len = draw.getByLabel(cs("draw.length"));
    // Shown in the UI language.
    await expect(len).toHaveValue("5,00");
    await typeInto(page, len, "5abc");
    await page.keyboard.press("ArrowRight");
    await expect(len).toHaveAttribute("aria-invalid", "true");
    await expect(draw).toContainText(cs("draw.points", { n: 1 }));
    // A press on a direction button does not draw the refused text either.
    await draw.getByRole("button", { name: cs("draw.dirUp") }).click();
    await expect(draw).toContainText(cs("draw.points", { n: 1 }));
    // Spaces are ignored, as in every number field ("1 2,5" is 12.5).
    await typeInto(page, len, "1 2,5");
    await page.keyboard.press("ArrowRight");
    await expect(draw).toContainText(cs("draw.points", { n: 2 }));
    await typeInto(page, len, "6");
    await page.keyboard.press("ArrowUp");
    await typeInto(page, len, "12,5");
    await page.keyboard.press("ArrowLeft");
    await expect(draw).toContainText(cs("draw.points", { n: 4 }));
    await page.keyboard.press("Enter");
    const p = await waitForProject(id, (p) => p.parts.length === 2);
    const xs = p.parts[1].edges.map((e: any) => e.start.x).sort((a: number, b: number) => a - b);
    expect(xs[3] - xs[0]).toBeCloseTo(12.5, 3);
  });

  test("Escape in the wall length field and a second press of Building keep the footprint", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.draw").click();
    const draw = bar(page, "draw.title");
    await pressPlan(page, 13.5, 1);
    const len = draw.getByLabel(cs("draw.length"));
    await typeInto(page, len, "4");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowUp");
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    // Escape while typing restores the field; a second Escape leaves it.
    await typeInto(page, len, "7");
    await page.keyboard.press("Escape");
    await expect(len).toHaveValue("4,00");
    await page.keyboard.press("Escape");
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    // Building pressed again asks; No keeps drawing, Yes stops.
    await app.answerConfirm(false);
    await tool(page, "tool.draw").click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toContain(cs("draw.confirmCancel", { n: 3 }));
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    // Insert while drawing goes on drawing.
    await page.locator("body").press("Insert");
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    await app.answerConfirm(true);
    await tool(page, "tool.draw").click();
    await expect(draw).toHaveCount(0);
  });

  test("the eight direction buttons have their own names", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.draw").click();
    const names = await bar(page, "draw.title").locator(".arrows button").evaluateAll((bs) => bs.map((b) => b.getAttribute("aria-label")));
    expect(new Set(names).size).toBe(8);
    expect(names).toContain(cs("draw.dirLeft"));
    expect(names).toContain(cs("draw.dirDownRight"));
  });

  test("Gable then Eave brings back the wall's own pitch; 90° says to use the switch", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    const type = (key: string) => def.getByRole("radio", { name: cs(key) }).click();
    await selectWall(page, 1);
    await setField(def.getByLabel(cs("edge.pitch")), "50");
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 50);
    await type("edge.gable");
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 90);
    await selectWall(page, 0);
    await setField(def.getByLabel(cs("edge.pitch")), "35");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 35);
    await type("edge.gable");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 90);
    await selectWall(page, 1);
    await type("edge.eave");
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 50);
    // A pitch of 90° is refused with the reason, not a generic message.
    await selectWall(page, 2);
    const pitch = def.getByLabel(cs("edge.pitch"));
    await typeInto(page, pitch, "90");
    await page.keyboard.press("Enter");
    await expect(pitch).toHaveAttribute("title", cs("edge.pitch90"));
  });

  test("standard roof: no stale preview and no OK for values the roof cannot take, with the reason", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.standard").click();
    const dlg = page.getByRole("dialog", { name: cs("std.title") });
    await dlg.getByLabel(cs("std.add")).check();
    const ok = dlg.getByRole("button", { name: cs("common.ok"), exact: true });
    const area = dlg.locator(".preview .area");
    await expect(area).toBeVisible();
    await expect(ok).toBeEnabled();
    const pitch = dlg.getByLabel(cs("edge.pitch"));
    await typeInto(page, pitch, "95");
    await expect(area).toHaveCount(0);
    await expect(ok).toBeDisabled();
    await expect(dlg.locator(".preview .why")).toContainText(cs("edge.pitch"));
    await typeInto(page, pitch, "30");
    await expect(area).toBeVisible();
    await expect(ok).toBeEnabled();
    // Length typed in mm: refused by the field, no OK (ERR-009).
    const length = dlg.getByLabel(cs("std.length"));
    await typeInto(page, length, "2000000");
    await expect(length).toHaveAttribute("aria-invalid", "true");
    await expect(ok).toBeDisabled();
    await typeInto(page, length, "12");
    await expect(ok).toBeEnabled();
    // A preview the calculation refuses keeps OK disabled and says why.
    await page.route("**/calc", (r) => r.fulfill({ contentType: "application/json", body: JSON.stringify({ err: { code: "out_of_range", detail: "value out of range" } }) }));
    await typeInto(page, length, "13");
    await expect(dlg.locator(".preview .why")).toContainText(cs("err.out_of_range"));
    await expect(ok).toBeDisabled();
    await page.unroute("**/calc");
    await typeInto(page, length, "12");
    await expect(ok).toBeEnabled();
    await ok.click();
    await waitForProject(id, (p) => p.parts.length === 2);
  });
});

