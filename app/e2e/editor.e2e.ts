// Editor: toolbar, layout, keyboard shortcuts, undo / redo, saving, the dock
// tabs and every control of the roof definition bar, checked against the
// stored file and the core's own calculation.
import { bar, firstArea, openEditor, plan, selectWall, setField, shownArea, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const calcArea = async (id: string) => (await bridge<any>("calc", { project: await loadProject(id) })).totals.roof_area as number;

/** The area in the results tab matches the core's calculation of the saved file. */
async function areaMatchesCore(page: import("@playwright/test").Page, id: string) {
  await expect.poll(async () => Math.abs((await shownArea(page)) - (await calcArea(id))), { timeout: 8000 }).toBeLessThan(0.01);
}

test.describe("editor shell", () => {
  test("layout buttons, F3, fit and dimensions toggle", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const views = page.locator(".views .view");
    await expect(views).toHaveCount(2);
    await tool(page, "editor.plan").click();
    await expect(views).toHaveCount(1);
    await expect(page.locator(".view canvas")).toHaveCount(0);
    await tool(page, "editor.model").click();
    await expect(views).toHaveCount(1);
    await expect(page.locator(".view canvas")).toHaveCount(1);
    await expect(tool(page, "plan.fit")).toBeDisabled();
    await page.keyboard.press("F3");
    await expect(views).toHaveCount(2);
    await page.keyboard.press("F3");
    await expect(plan(page)).toHaveCount(0);
    await tool(page, "editor.both").click();
    await expect(views).toHaveCount(2);

    // The layout is remembered.
    await page.reload();
    await app.openJob("Sedlo");
    await expect(views).toHaveCount(2);

    const dims = page.locator("text.edge-dim");
    await expect(dims.first()).toBeVisible();
    await tool(page, "tool.dims").click();
    await expect(dims).toHaveCount(0);
    await tool(page, "tool.dims").click();
    await expect(dims).toHaveCount(4);

    // Zoom out with the wheel, then Fit brings the walls back to a readable size.
    const box = (await plan(page).boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    for (let i = 0; i < 20; i++) await page.mouse.wheel(0, 400);
    // At the smallest zoom only the 12 m walls are long enough for a label.
    await expect(dims).toHaveCount(2);
    await tool(page, "plan.fit").click();
    await expect(dims).toHaveCount(4);
  });

  test("rename in the title, Ctrl+S and leaving the editor keep the change", async ({ app, page }) => {
    const id = await createExample("gable", "Staré jméno");
    await openEditor(app, "Staré jméno");
    const title = page.locator(".editor input.title");
    await title.fill("Nové jméno");
    await title.press("Enter");
    await app.saved();
    await waitForProject(id, (p) => p.name === "Nové jméno");
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await expect(page.getByRole("row").filter({ hasText: "Nové jméno" })).toBeVisible();
  });

  test("undo / redo with the buttons and with Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y and Cmd+Z", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const undo = tool(page, "editor.undo");
    const redo = tool(page, "editor.redo");
    await expect(undo).toBeDisabled();
    await expect(redo).toBeDisabled();
    await selectWall(page, 0);
    const pitch = bar(page, "def.title").getByLabel(cs("edge.pitch"));
    await setField(pitch, 30);
    await setField(pitch, 25);
    await expect(undo).toBeEnabled();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 25);

    await undo.click();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 30);
    await page.locator("body").press("Control+z");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 35);
    await expect(undo).toBeDisabled();
    await redo.click();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 30);
    await page.locator("body").press("Control+Shift+z");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 25);
    await expect(redo).toBeDisabled();
    await page.locator("body").press("Meta+z");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 30);
    await page.locator("body").press("Control+y");
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 25);
    await page.locator("body").press("Control+s");
    await app.saved();
    await areaMatchesCore(page, id);
  });

  test("dock tabs switch between results, AI and job", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const tabs = page.locator("aside .tabs");
    await expect(page.locator(".results")).toBeVisible();
    await tabs.getByRole("tab", { name: cs("dock.ai") }).click();
    await expect(page.getByText(cs("ai.history"))).toBeVisible();
    await tabs.getByRole("tab", { name: cs("dock.job") }).click();
    await expect(page.locator("aside .job").getByLabel(cs("job.name"))).toHaveValue("Sedlo");
    await tabs.getByRole("tab", { name: cs("dock.results") }).click();
    await expect(page.locator(".results .hero strong")).not.toHaveText("—");
    // The AI pill in the header opens the AI tab.
    await page.locator("header .ai-pill").click();
    await expect(tabs.getByRole("tab", { name: cs("dock.ai") })).toHaveAttribute("aria-selected", "true");
  });

  test("status bar shows the cursor position, walls and area", async ({ app, page }) => {
    await createExample("l-shape", "Elko");
    await openEditor(app, "Elko");
    await expect(page.locator(".statusbar")).toContainText(cs("status.walls", { n: 6, p: 1 }));
    const box = (await plan(page).boundingBox())!;
    await page.mouse.move(box.x + 50, box.y + 50);
    await expect(page.locator(".statusbar .coords")).not.toContainText("—");
    await page.mouse.move(box.x - 20, box.y - 20);
    await expect(page.locator(".statusbar .coords")).toContainText("—");
  });
});

test.describe("roof definition bar", () => {
  test("previous / next step through every wall, Escape clears the selection", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const def = bar(page, "def.title");
    await expect(def).toContainText(cs("def.none"));
    await def.getByRole("button", { name: cs("def.next") }).click();
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 1 }));
    await def.getByRole("button", { name: cs("def.prev") }).click();
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 4 }));
    await def.getByRole("button", { name: cs("def.next") }).click();
    await def.getByRole("button", { name: cs("def.next") }).click();
    await expect(def.locator(".which strong")).toHaveText(cs("def.wall", { n: 2 }));
    await expect(def.getByRole("radio", { name: cs("edge.gable") })).toHaveAttribute("aria-checked", "true");
    await page.keyboard.press("Escape");
    await expect(def).toContainText(cs("def.none"));
    // Clicking the plan background clears it too.
    await selectWall(page, 0);
    const box = (await plan(page).boundingBox())!;
    await page.mouse.click(box.x + 8, box.y + 8);
    await expect(def).toContainText(cs("def.none"));
  });

  test("eave / gable toggle, pitch, eave height and overhang change the roof", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const def = bar(page, "def.title");
    const before = await firstArea(page);

    await selectWall(page, 1);
    await def.getByRole("radio", { name: cs("edge.eave") }).click();
    let p = await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg !== 90);
    expect(p.parts[0].edges[1].pitch_deg).toBe(40);
    await expect(page.locator(".results")).toContainText("G");
    await areaMatchesCore(page, id);
    await def.getByRole("radio", { name: cs("edge.gable") }).click();
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 90);
    await expect(def.getByLabel(cs("edge.pitch"))).toBeDisabled();

    await selectWall(page, 0);
    await setField(def.getByLabel(cs("edge.pitch")), "35");
    await setField(def.getByLabel(cs("edge.eaveHeight")), "3.5");
    await setField(def.getByLabel(cs("edge.overhang")), "0.8");
    p = await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.8);
    expect(p.parts[0].edges[0]).toMatchObject({ pitch_deg: 35, eave_height: 3.5, overhang: 0.8 });
    await areaMatchesCore(page, id);
    expect(await shownArea(page)).not.toBeCloseTo(before, 1);

    // Out-of-range input is ignored, not stored.
    await setField(def.getByLabel(cs("edge.pitch")), "0");
    await setField(def.getByLabel(cs("edge.overhang")), "-1");
    await page.waitForTimeout(800);
    expect((await loadProject(id)).parts[0].edges[0]).toMatchObject({ pitch_deg: 35, overhang: 0.8 });
  });

  test("length and start point move the corners", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await expect(def.getByLabel(cs("edge.length"))).toHaveValue("12,00");
    await setField(def.getByLabel(cs("edge.length")), "14");
    let p = await waitForProject(id, (p) => p.parts[0].edges[1].start.x === 14);
    expect(p.parts[0].edges[1].start).toEqual({ x: 14, y: 0 });
    await setField(def.getByLabel(cs("edge.startX")), "-1");
    await setField(def.getByLabel(cs("edge.startY")), "-0.5");
    p = await waitForProject(id, (p) => p.parts[0].edges[0].start.y === -0.5);
    expect(p.parts[0].edges[0].start).toEqual({ x: -1, y: -0.5 });
    await areaMatchesCore(page, id);
  });

  test("apply pitch, height and material to all walls", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 2);
    await setField(def.getByLabel(cs("edge.pitch")), 28);
    await setField(def.getByLabel(cs("edge.eaveHeight")), 4.2);
    await setField(def.getByLabel(cs("edge.overhang")), 0.3);
    await def.getByLabel(cs("edge.material")).selectOption("3");
    await def.getByRole("button", { name: cs("edge.pitchAll") }).click();
    await def.getByRole("button", { name: cs("edge.heightAll") }).click();
    await def.getByRole("button", { name: cs("edge.materialAll") }).click();
    const p = await waitForProject(id, (p) => p.parts[0].edges.every((e: any) => e.material === 3));
    for (const e of p.parts[0].edges) expect(e).toMatchObject({ pitch_deg: 28, eave_height: 4.2, overhang: 0.3, material: 3 });
    await areaMatchesCore(page, id);
    // One material that is not the default: the area by material is listed.
    await expect(page.locator(".results")).toContainText(cs("res.byMaterial"));
  });

  test("pitch to all eaves leaves gables alone", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await setField(def.getByLabel(cs("edge.pitch")), 33);
    await def.getByRole("button", { name: cs("edge.pitchAll") }).click();
    const p = await waitForProject(id, (p) => p.parts[0].edges[2].pitch_deg === 33);
    expect(p.parts[0].edges.map((e: any) => e.pitch_deg)).toEqual([33, 90, 33, 90]);
  });

  test("gable options: clad gable, half-hip with height and pitch", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const def = bar(page, "def.title");
    const a0 = await firstArea(page);
    await selectWall(page, 1);
    await def.getByLabel(cs("edge.clad")).check();
    await waitForProject(id, (p) => p.parts[0].edges[1].clad === true);
    await expect(def.getByLabel(cs("edge.material"))).toBeVisible();
    await areaMatchesCore(page, id);
    expect(await shownArea(page)).toBeGreaterThan(a0);
    await def.getByLabel(cs("edge.clad")).uncheck();

    await def.getByLabel(cs("edge.halfHip"), { exact: true }).check();
    await waitForProject(id, (p) => !!p.parts[0].edges[1].half_hip);
    await setField(def.getByLabel(cs("edge.halfHipHeight")), 6);
    await setField(def.getByLabel(cs("edge.halfHipPitch")), 55);
    const p = await waitForProject(id, (p) => p.parts[0].edges[1].half_hip?.pitch_deg === 55);
    expect(p.parts[0].edges[1].half_hip).toEqual({ height: 6, pitch_deg: 55 });
    await areaMatchesCore(page, id);
    await def.getByLabel(cs("edge.halfHip"), { exact: true }).uncheck();
    await waitForProject(id, (p) => !p.parts[0].edges[1].half_hip);
  });

  test("pitch breaks: add, edit and remove (mansard)", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await def.getByRole("button", { name: cs("edge.addBreak") }).click();
    let p = await waitForProject(id, (p) => p.parts[0].edges[0].breaks?.length === 1);
    // 1.5 m above the eave, 20° flatter than the wall (hip example: 35°).
    expect(p.parts[0].edges[0].breaks[0]).toEqual({ height: 4.5, pitch_deg: 15 });
    await setField(def.getByLabel(cs("edge.breakHeight", { n: 1 })), 4);
    await setField(def.getByLabel(cs("edge.breakPitch")), 30);
    p = await waitForProject(id, (p) => p.parts[0].edges[0].breaks?.[0]?.pitch_deg === 30);
    expect(p.parts[0].edges[0].breaks[0]).toEqual({ height: 4, pitch_deg: 30 });
    await expect(page.locator(".results")).toContainText(cs("res.knick"));
    await areaMatchesCore(page, id);
    await def.getByRole("button", { name: cs("edge.removeBreak") }).click();
    await waitForProject(id, (p) => !p.parts[0].edges[0].breaks?.length);
  });

  test("priority flag is stored", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await selectWall(page, 0);
    await bar(page, "def.title").getByLabel(cs("edge.priority")).check();
    await waitForProject(id, (p) => p.parts[0].edges[0].priority === 1);
    await bar(page, "def.title").getByLabel(cs("edge.priority")).uncheck();
    await waitForProject(id, (p) => !p.parts[0].edges[0].priority);
  });

  test("direction change: insert, set direction, flip, delete", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const def = bar(page, "def.title");
    await selectWall(page, 1);
    await def.getByRole("button", { name: cs("edge.addMarker") }).click();
    let p = await waitForProject(id, (p) => p.parts[0].edges.length === 5);
    expect(p.parts[0].edges[1].direction_rad).toBeCloseTo(Math.PI, 6);
    await expect(def.locator(".kind")).toHaveText(cs("edge.marker"));
    await setField(def.getByRole("textbox", { name: new RegExp(`^${cs("edge.direction")}`) }), 170);
    p = await waitForProject(id, (p) => Math.abs(p.parts[0].edges[1].direction_rad - (170 * Math.PI) / 180) < 1e-9);
    await def.getByRole("button", { name: cs("edge.flip") }).click();
    p = await waitForProject(id, (p) => Math.abs(p.parts[0].edges[1].direction_rad - (350 * Math.PI) / 180) < 1e-9);
    await areaMatchesCore(page, id);
    await def.getByRole("button", { name: cs("edge.deleteVertex") }).click();
    await waitForProject(id, (p) => p.parts[0].edges.length === 4);
  });

  test("delete a corner with the button and the Delete key, never below three walls", async ({ app, page }) => {
    const id = await createExample("l-shape", "Elko");
    await openEditor(app, "Elko");
    const def = bar(page, "def.title");
    await selectWall(page, 3);
    await def.getByRole("button", { name: cs("edge.deleteVertex") }).click();
    await waitForProject(id, (p) => p.parts[0].edges.length === 5);
    await selectWall(page, 0);
    await page.keyboard.press("Delete");
    await waitForProject(id, (p) => p.parts[0].edges.length === 4);
    await selectWall(page, 0);
    await page.keyboard.press("Delete");
    await waitForProject(id, (p) => p.parts[0].edges.length === 3);
    await selectWall(page, 0);
    await expect(def.getByRole("button", { name: cs("edge.deleteVertex") })).toBeDisabled();
    await page.keyboard.press("Delete");
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts[0].edges.length).toBe(3);
  });

  test("dragging a corner moves it and is one undo step", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const v = page.locator("circle.vertex").nth(2);
    const box = (await v.boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 60, box.y - 40, { steps: 5 });
    await page.mouse.up();
    const p = await waitForProject(id, (p) => p.parts[0].edges[2].start.x !== 12);
    expect(p.parts[0].edges[2].start.x).toBeGreaterThan(12);
    await areaMatchesCore(page, id);
    await tool(page, "editor.undo").click();
    await waitForProject(id, (p) => p.parts[0].edges[2].start.x === 12 && p.parts[0].edges[2].start.y === 8);
  });

  test("double-click on a wall inserts a corner", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const a = (await page.locator("circle.vertex").nth(0).boundingBox())!;
    const b = (await page.locator("circle.vertex").nth(1).boundingBox())!;
    await page.mouse.dblclick((a.x + b.x) / 2 + a.width / 2, (a.y + b.y) / 2 + a.height / 2);
    await waitForProject(id, (p) => p.parts[0].edges.length === 5);
  });

  test("calculation helper and round roof dialogs: cancel changes nothing, OK applies", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await def.getByRole("button", { name: cs("calc.button") }).click();
    const calc = page.getByRole("dialog", { name: cs("calc.title") });
    await expect(calc).toBeVisible();
    await calc.getByRole("button", { name: cs("common.cancel") }).click();
    await expect(calc).toHaveCount(0);

    await def.getByRole("button", { name: cs("round.button") }).click();
    const round = page.getByRole("dialog", { name: cs("round.title") });
    await expect(round).toBeVisible();
    await expect(round.getByRole("button", { name: cs("common.ok") })).toBeEnabled();
    await round.getByRole("button", { name: cs("common.ok") }).click();
    const p = await waitForProject(id, (p) => (p.parts[0].edges[0].breaks?.length ?? 0) > 0);
    expect(p.parts[0].edges[2].breaks.length).toBe(p.parts[0].edges[0].breaks.length);
    await areaMatchesCore(page, id);
    const stored = (await listProjects())[0];
    expect(stored.roof_area).toBeGreaterThan(0);
  });
});

test.describe("keyboard", () => {
  test("Backspace deletes like Delete (macOS has no Delete key)", async ({ app, page }) => {
    const id = await createExample("l-shape", "Elko");
    await openEditor(app, "Elko");
    await selectWall(page, 0);
    await page.keyboard.press("Backspace");
    await waitForProject(id, (p) => p.parts[0].edges.length === 5);
  });

  test("keys pressed while a dialog is open do not edit the roof behind it", async ({ app, page }) => {
    const id = await createExample("l-shape", "Elko");
    await openEditor(app, "Elko");
    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    const dlg = page.getByRole("dialog", { name: cs("xf.title") });
    await dlg.getByRole("radio", { name: cs("xf.rotate") }).focus();
    await page.keyboard.press("Delete");
    await page.keyboard.press("Control+z");
    await page.keyboard.press("Escape");
    await expect(dlg).toHaveCount(0);
    // The wall is still selected and nothing was deleted.
    await expect(bar(page, "def.title").locator(".which strong")).toHaveText(cs("def.wall", { n: 1 }));
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts[0].edges).toHaveLength(6);
  });

  test("shortcut hints use the platform's modifier key", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const mac = await page.evaluate(() => /Mac/.test(navigator.platform));
    await expect(tool(page, "editor.undo")).toHaveAttribute("title", `${cs("editor.undo")} (${mac ? "⌘Z" : "Ctrl+Z"})`);
  });
});
