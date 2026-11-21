// Guards for the round-3 audit (2026-11-21) findings in the editor bars and
// dialogs: a value typed in the chimney / opening bar stays on its object
// when ▶ or PageDown moves on (UI-202); an editor action whose core answer
// comes after an AI save still lands (ERR-203); the button that appears
// under the pointer after Close face ignores the double click's second
// press in German (UI-208); quick arrow steps in the dormer bar all land
// (UI-201); the dormer dialog checks the round dormer's segments and its
// OK clears an old refusal (UI-203, UI-207); the bar takes 0 segments as
// the hint says (UI-204); Gable, Undo, Redo, Eave keeps the wall's own
// pitch (UX-202); a recalculation on its way is marked (ERR-204).
import type { Locator, Page, Route } from "@playwright/test";
import { bar, clickPlan, openEditor, planXY, selectWall, setField, tool } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs, tr } from "./i18n";

const square = (x: number, y: number, s: number) => [
  { x, y },
  { x: x + s, y },
  { x: x + s, y: y + s },
  { x, y: y + s },
];
const extentX = (o: any) => +(Math.max(...o.corners.map((c: any) => c.x)) - Math.min(...o.corners.map((c: any) => c.x))).toFixed(3);
const dormerWidth = (d: any) => +Math.hypot(d.end.x - d.edges[0].start.x, d.end.y - d.edges[0].start.y).toFixed(3);

async function withItems(id: string, edit: (p: any) => void) {
  const p = await loadProject(id);
  edit(p);
  await bridge("save_project", { id, project: p });
}

/** Types like a person: click, select all, type (no fill()). */
async function typeInto(page: Page, field: Locator, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

/** Holds back the bridge's answers to `cmd` by `ms` while `on()` is true. */
async function slow(page: Page, cmd: string, ms: number, on: () => boolean) {
  await page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    if (on()) await new Promise((res) => setTimeout(res, ms));
    await r.continue();
  });
}

const CORS = { "access-control-allow-origin": "*" };

test.describe("round-3 audit: bars and dialogs", () => {
  for (const kind of ["chimney", "hole"] as const) {
    test(`a width typed in the ${kind} bar stays on its object when ▶ or PageDown moves on (UI-202)`, async ({ app, page }) => {
      const id = await createExample("gable", `Objekty ${kind}`);
      await withItems(id, (p) => {
        p.chimneys = [
          { corners: square(3, 1, 0.6), height: 8 },
          { corners: square(8, 1, 0.6), height: 8 },
        ];
        p.roof_openings = [
          { corners: square(3, 5.5, 1), height: 0 },
          { corners: square(8, 5.5, 1), height: 0 },
        ];
      });
      await openEditor(app, `Objekty ${kind}`);
      const list = (p: any) => (kind === "chimney" ? p.chimneys : p.roof_openings);
      const ob = bar(page, `opening.${kind}`);
      const first = async () => {
        await page.keyboard.press("Escape");
        const bb = (await page.locator(`path.opening.${kind}`).nth(0).boundingBox())!;
        await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
        await expect(ob.locator(".which strong")).toHaveText(cs(`opening.${kind}N`, { n: 1 }));
      };
      const other = extentX(list(await loadProject(id))[1]);

      await first();
      await typeInto(page, ob.getByLabel(cs("opening.width")), "0,71");
      await ob.getByRole("button", { name: cs("opening.next") }).click();
      await expect(ob.locator(".which strong")).toHaveText(cs(`opening.${kind}N`, { n: 2 }));
      await waitForProject(id, (p) => extentX(list(p)[0]) === 0.71);
      expect(extentX(list(await loadProject(id))[1])).toBe(other);

      await first();
      await typeInto(page, ob.getByLabel(cs("opening.width")), "0,73");
      await page.keyboard.press("PageDown");
      await expect(ob.locator(".which strong")).toHaveText(cs(`opening.${kind}N`, { n: 2 }));
      await waitForProject(id, (p) => extentX(list(p)[0]) === 0.73);
      expect(extentX(list(await loadProject(id))[1])).toBe(other);
    });
  }

  test("a chimney distance whose answer comes after an AI save still lands (ERR-203)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Komín a AI");
    await withItems(id, (p) => (p.chimneys = [{ corners: square(5, 1.5, 0.6), height: 8 }]));
    await openEditor(app, "Komín a AI");
    const bb = (await page.locator("path.opening.chimney").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    const ob = bar(page, "opening.chimney");
    let delay = true;
    await slow(page, "place_item", 2500, () => delay);
    await ob.getByTestId("place-eave").fill("2");
    await ob.getByTestId("place-eave").press("Enter");
    await page.waitForTimeout(500);
    await mcp.call("update_job", { id, notes: "AI note", summary: "AI adds a note." });
    await waitForProject(id, (p) => p.notes === "AI note");
    delay = false;
    await expect
      .poll(async () => +(await bridge<any>("item_placement", { project: await loadProject(id), kind: "chimney", index: 0 })).from_eave.toFixed(3), { timeout: 10_000 })
      .toBe(2);
    expect((await loadProject(id)).notes).toBe("AI note");
  });

  test("Duplicate building whose answer comes after an AI save still makes the copy (ERR-203)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Kopie a AI");
    await openEditor(app, "Kopie a AI");
    let delay = true;
    await slow(page, "transform_part", 2500, () => delay);
    const at = await planXY(page, 3, 3);
    await page.mouse.click(at.x, at.y, { button: "right" });
    await page.getByRole("menuitem", { name: cs("menu.duplicatePart") }).click();
    await page.waitForTimeout(500);
    await mcp.call("update_job", { id, notes: "AI note", summary: "AI adds a note." });
    await waitForProject(id, (p) => p.notes === "AI note");
    delay = false;
    const p = await waitForProject(id, (p) => p.parts.length === 2, 10_000);
    expect(p.notes).toBe("AI note");
  });

  test("an action whose own building the AI changed meanwhile is refused with a note, not dropped (ERR-203)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Kopie a změna");
    await openEditor(app, "Kopie a změna");
    let delay = true;
    await slow(page, "transform_part", 2500, () => delay);
    const at = await planXY(page, 3, 3);
    await page.mouse.click(at.x, at.y, { button: "right" });
    await page.getByRole("menuitem", { name: cs("menu.duplicatePart") }).click();
    await page.waitForTimeout(500);
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "AI sets the pitch." });
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 30);
    delay = false;
    await expect(app.toasts().filter({ hasText: cs("editor.actionStale") })).toBeVisible({ timeout: 10_000 });
    expect((await loadProject(id)).parts).toHaveLength(1);
  });

  test("a double click on Close face in German leaves the face a face (UI-208)", async ({ app, page }) => {
    const id = await createExample("hip", "Plocha de");
    await app.open({ locale: "de" });
    await app.openJob("Plocha de");
    // German toolbar: the face tool by its German label.
    await page.getByRole("toolbar").first().getByRole("button", { name: tr("de", "tool.face"), exact: true }).click();
    for (const [x, y] of [
      [2, 1],
      [6, 1],
      [6, 3],
    ])
      await clickPlan(page, x, y);
    const close = page.getByTestId("place-bar").getByRole("button", { name: tr("de", "place.closeFace") });
    const bb = (await close.boundingBox())!;
    const cx = bb.x + bb.width / 2;
    const cy = bb.y + bb.height / 2;
    // Two separate presses at the same spot, as a quick double click.
    await page.mouse.click(cx, cy);
    await page.waitForTimeout(80);
    await page.mouse.click(cx, cy);
    await waitForProject(id, (p) => (p.manual_faces ?? []).length === 1);
    await page.waitForTimeout(1200);
    const p = await loadProject(id);
    expect(p.parts).toHaveLength(1);
    expect(p.manual_faces ?? []).toHaveLength(1);
    // A real press on the bar's button later still works.
    await page.getByRole("button", { name: tr("de", "manual.toPart") }).click();
    await waitForProject(id, (p) => p.parts.length === 2);
  });

  test("quick ArrowUp steps in the dormer bar all land while the check is slow (UI-201)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Kroky vikýře");
    await openEditor(app, "Kroky vikýře");
    await mcp.call("add_dormer", { id, kind: "gable", start: [4, 1], summary: "Dormer." });
    await expect(page.locator("g.dormer")).toHaveCount(1);
    await page.getByRole("button", { name: cs("review.accept") }).first().click();
    const db = bar(page, "dormer.title");
    const bb = (await page.locator("g.dormer path.dormer-area").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 1 }));
    const w0 = dormerWidth((await loadProject(id)).dormers[0]);
    await slow(page, "check_dormer", 400, () => true);
    const field = db.getByLabel(cs("dormer.width"));
    await field.click();
    for (let i = 0; i < 4; i++) await field.press("ArrowUp");
    await waitForProject(id, (p) => dormerWidth(p.dormers[0]) === +(w0 + 0.2).toFixed(3), 10_000);
    await expect(field).not.toHaveAttribute("aria-invalid", "true");
  });

  test("a width then a pitch typed while the dormer check is slow both land (UI-201)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Dvě pole vikýře");
    await openEditor(app, "Dvě pole vikýře");
    await mcp.call("add_dormer", { id, kind: "gable", start: [4, 1], summary: "Dormer." });
    await expect(page.locator("g.dormer")).toHaveCount(1);
    await page.getByRole("button", { name: cs("review.accept") }).first().click();
    const db = bar(page, "dormer.title");
    const bb = (await page.locator("g.dormer path.dormer-area").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    await slow(page, "check_dormer", 600, () => true);
    await typeInto(page, db.getByLabel(cs("dormer.width")), "2,7");
    await page.keyboard.press("Tab");
    await typeInto(page, db.getByRole("textbox", { name: `${cs("edge.pitch")} °`, exact: true }), "30");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => dormerWidth(p.dormers[0]) === 2.7 && p.dormers[0].edges.some((e: any) => e.pitch_deg === 30), 10_000);
  });

  const newDormer = async (app: import("./fixtures").App, page: Page, name: string) => {
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await tool(page, "tool.dormer").click();
    await clickPlan(page, 5, 2.4);
    const dlg = page.getByRole("dialog", { name: cs("dormer.dialogNew") });
    await expect(dlg).toBeVisible();
    await page.waitForTimeout(800);
    return { id, dlg, ok: dlg.getByRole("button", { name: cs("common.ok"), exact: true }) };
  };

  test("the dormer dialog refuses a round dormer with too many segments for its width (UI-203)", async ({ app, page }) => {
    const { id, dlg, ok } = await newDormer(app, page, "Kulatý vikýř");
    await dlg.getByRole("radio", { name: cs("dormer.round") }).click();
    // Many segments on a wide dormer are fine; a narrower width makes them too flat.
    await setField(dlg.getByLabel(cs("dormer.width")), "4");
    await setField(dlg.getByLabel(cs("round.segments")), "10");
    await expect(dlg.getByLabel(cs("round.segments"))).toHaveValue("10");
    await expect(dlg.getByTestId("dormer-refusal")).toHaveCount(0);
    // The width field asks the core like the pitch fields.
    await setField(dlg.getByLabel(cs("dormer.width")), "0,6");
    await expect(dlg.getByTestId("dormer-refusal")).toBeVisible();
    await expect(ok).toBeDisabled();
    // A way round the fields' checks: narrow it as a gable, then make it round
    // again (the kind tiles store at once). OK asks the core once more.
    await dlg.getByRole("radio", { name: cs("dormer.gable") }).click();
    await setField(dlg.getByLabel(cs("dormer.width")), "0,6");
    await dlg.getByRole("radio", { name: cs("dormer.round") }).click();
    await expect(ok).toBeEnabled();
    await ok.click();
    await expect(page.getByTestId("dormer-ok-blocked")).toBeVisible();
    await expect(dlg).toBeVisible();
    await page.waitForTimeout(500);
    expect((await loadProject(id)).dormers).toHaveLength(0);
    // Fewer segments: OK stores it.
    await setField(dlg.getByLabel(cs("round.segments")), "2");
    await expect(ok).toBeEnabled();
    await ok.click();
    await expect(dlg).toBeHidden();
    await waitForProject(id, (p) => p.dormers.length === 1);
  });

  test("the dormer bar takes 0 segments, the usual count its hint names (UI-204)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Nula úseků");
    await openEditor(app, "Nula úseků");
    await mcp.call("add_dormer", { id, kind: "round", start: [4, 1], summary: "Round dormer." });
    await expect(page.locator("g.dormer")).toHaveCount(1);
    await page.getByRole("button", { name: cs("review.accept") }).first().click();
    const db = bar(page, "dormer.title");
    const bb = (await page.locator("g.dormer path.dormer-area").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    const segs = db.getByLabel(cs("round.segments"));
    await setField(segs, "0");
    // Read at once: a refusal marks the field only for a moment.
    expect(await segs.getAttribute("aria-invalid")).toBeNull();
    await page.waitForTimeout(600);
    expect(await segs.getAttribute("aria-invalid")).toBeNull();
  });

  test("the dormer dialog's OK is enabled again once the refused value is typed back to a storable one (UI-207)", async ({ app, page }) => {
    const { dlg, ok } = await newDormer(app, page, "Zpět platný");
    const eave = dlg.getByLabel(cs("dormer.eaveAbs"));
    const original = await eave.inputValue();
    await typeInto(page, eave, "1");
    await page.keyboard.press("Enter");
    await expect(dlg.getByTestId("dormer-refusal")).toBeVisible();
    await expect(ok).toBeDisabled();
    await typeInto(page, eave, original);
    await page.keyboard.press("Enter");
    await expect(ok).toBeEnabled();
    await expect(dlg.getByTestId("dormer-refusal")).toHaveCount(0);
    app.errors.length = 0;
  });

  test("Gable, Undo, Redo, then Eave gives the wall its own pitch back (UX-202)", async ({ app, page }) => {
    const id = await createExample("hip", "Zpět a znovu");
    await openEditor(app, "Zpět a znovu");
    const def = bar(page, "def.title");
    const type = (key: string) => def.getByRole("radio", { name: cs(key) }).click();
    await selectWall(page, 1);
    await setField(def.getByLabel(cs("edge.pitch")), "50");
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 50);
    await type("edge.gable");
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 90);
    await page.locator(".view .wrap > svg").first().focus();
    await page.keyboard.press("ControlOrMeta+z");
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 50);
    await page.keyboard.press("ControlOrMeta+Shift+z");
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 90);
    await selectWall(page, 1);
    await type("edge.eave");
    const p = await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg !== 90);
    expect(p.parts[0].edges[1].pitch_deg).toBe(50);
  });

  test("Results and the status bar mark a recalculation on its way (ERR-204)", async ({ app, page }) => {
    await createExample("gable", "Počítá se");
    let delay = false;
    await slow(page, "calc", 2500, () => delay);
    await openEditor(app, "Počítá se");
    await expect(page.locator(".results .hero strong")).not.toHaveText("—");
    await expect(page.getByTestId("calc-pending")).toHaveCount(0);
    delay = true;
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await setField(def.getByLabel(cs("edge.pitch")), "20");
    await expect(page.getByTestId("calc-pending")).toBeVisible();
    await expect(page.getByTestId("res-stale")).toHaveText(cs("res.calculating"));
    await expect(page.getByTestId("status-area")).toHaveAttribute("aria-busy", "true");
    delay = false;
    await expect(page.getByTestId("calc-pending")).toHaveCount(0, { timeout: 10_000 });
    await expect(page.getByTestId("res-stale")).toHaveCount(0);
    await expect(page.getByTestId("status-area")).not.toHaveAttribute("aria-busy", "true");
    void CORS;
  });
});
