// Objects on the roof: dormers, chimneys, roof windows, roof openings,
// hand-drawn lines and faces, flat roof, part transform and delete, and the
// 3D view buttons. Each is placed with its tool, edited in its bar, deleted,
// and checked against the stored file and the core calculation.
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, selectWall, setField, shownArea, tool } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

async function areaMatchesCore(page: Page, id: string) {
  await expect
    .poll(async () => {
      const r = await bridge<any>("calc", { project: await loadProject(id) });
      return Math.abs((await shownArea(page)) - r.totals.roof_area);
    })
    .toBeLessThan(0.01);
}

test.describe("dormers", () => {
  test("place, change every kind and its fields, step, delete with the button and the Delete key", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const a0 = await shownArea(page);
    await tool(page, "tool.dormer").click();
    await expect(page.getByText(cs("plan.placeDormer"))).toBeVisible();
    await clickPlan(page, 4, 1);
    const db = bar(page, "dormer.title");
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 1 }));
    let p = await waitForProject(id, (p) => p.dormers.length === 1);
    expect(p.dormers[0].edges.length).toBeGreaterThan(2);
    await areaMatchesCore(page, id);
    expect(await shownArea(page)).not.toBeCloseTo(a0, 2);

    await setField(db.getByLabel(cs("dormer.width")), 2.4);
    await setField(db.getByLabel(cs("edge.pitch")), 38);
    await setField(db.getByLabel(cs("edge.overhang")), 0.2);
    await db.getByLabel(cs("edge.material")).selectOption("2");
    await areaMatchesCore(page, id);

    for (const kind of ["hip", "half_hip", "shed", "trapezoid", "pointed", "round", "gable"]) {
      await db.getByLabel(cs("dormer.kind")).selectOption(kind);
      await expect(db.getByLabel(cs("dormer.kind"))).toHaveValue(kind);
      await areaMatchesCore(page, id);
    }
    p = await loadProject(id);
    expect(p.dormers).toHaveLength(1);

    // A second dormer, then step between them.
    await tool(page, "tool.dormer").click();
    await clickPlan(page, 8, 1);
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 2 }));
    await db.getByRole("button", { name: cs("dormer.prev") }).click();
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 1 }));
    await db.getByRole("button", { name: cs("dormer.next") }).click();
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 2 }));

    await db.getByRole("button", { name: cs("dormer.delete") }).click();
    await waitForProject(id, (p) => p.dormers.length === 1);
    await page.locator("g.dormer line.hit").first().dispatchEvent("pointerdown", { button: 0 });
    await expect(db).toBeVisible();
    await page.keyboard.press("Delete");
    await waitForProject(id, (p) => p.dormers.length === 0);
    await areaMatchesCore(page, id);
  });

  test("Escape and a second click on the tool cancel placing", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    for (const key of ["tool.dormer", "tool.chimney", "tool.window", "tool.hole", "tool.line", "tool.face"]) {
      await tool(page, key).click();
      await expect(tool(page, key)).toHaveClass(/\bon\b/);
      await page.keyboard.press("Escape");
      await expect(tool(page, key)).not.toHaveClass(/\bon\b/);
      await tool(page, key).click();
      await tool(page, key).click();
      await expect(tool(page, key)).not.toHaveClass(/\bon\b/);
    }
    await page.waitForTimeout(600);
    const p = await loadProject(id);
    expect([p.dormers.length, p.chimneys.length, p.windows?.length ?? 0, p.roof_openings?.length ?? 0]).toEqual([0, 0, 0, 0]);
  });

  test("object tools are disabled with no roof", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByRole("tab", { name: cs("new.draw") }).click();
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    for (const key of ["tool.dormer", "tool.chimney", "tool.window", "tool.hole", "tool.flat", "tool.transform", "tool.deletePart", "tool.clear"]) {
      await expect(tool(page, key), key).toBeDisabled();
    }
  });
});

test.describe("chimneys, windows and openings", () => {
  test("chimney: place, edit position, size, rotation and top, delete", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await tool(page, "tool.chimney").click();
    await clickPlan(page, 5, 3);
    const ob = bar(page, "opening.chimney");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.chimneyN", { n: 1 }));
    let p = await waitForProject(id, (p) => p.chimneys.length === 1);
    expect(p.chimneys[0].corners[0]).toEqual({ x: 5, y: 3 });
    await areaMatchesCore(page, id);

    await setField(ob.getByRole("spinbutton", { name: /^X\b/ }), 5.5);
    await setField(ob.getByRole("spinbutton", { name: /^Y\b/ }), 3.5);
    await setField(ob.getByLabel(cs("opening.width")), 0.9);
    await setField(ob.getByLabel(cs("opening.depth")), 0.7);
    await setField(ob.getByLabel(cs("opening.rotation")), 0);
    await setField(ob.getByLabel(cs("opening.top")), 9);
    p = await waitForProject(id, (p) => p.chimneys[0].height === 9);
    expect(p.chimneys[0].corners).toEqual([
      { x: 5.5, y: 3.5 },
      { x: 6.4, y: 3.5 },
      { x: 6.4, y: 4.2 },
      { x: 5.5, y: 4.2 },
    ]);
    await areaMatchesCore(page, id);
    await ob.getByRole("button", { name: cs("opening.delete") }).click();
    await waitForProject(id, (p) => p.chimneys.length === 0);
  });

  test("roof window: place, edit, step between two, delete with Delete", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await tool(page, "tool.window").click();
    await clickPlan(page, 3, 1.5);
    await tool(page, "tool.window").click();
    await clickPlan(page, 7, 1.5);
    const ob = bar(page, "opening.window");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.windowN", { n: 2 }));
    await setField(ob.getByLabel(cs("opening.width")), 1.14);
    await setField(ob.getByLabel(cs("opening.height")), 1.4);
    let p = await waitForProject(id, (p) => p.windows?.[1]?.height === 1.4);
    expect(p.windows[1]).toMatchObject({ x: 7, y: 1.5, width: 1.14, height: 1.4 });
    await areaMatchesCore(page, id);
    await ob.getByRole("button", { name: cs("opening.prev") }).click();
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.windowN", { n: 1 }));
    await page.keyboard.press("Delete");
    p = await waitForProject(id, (p) => p.windows?.length === 1);
    expect(p.windows[0].x).toBe(7);
  });

  test("roof opening: place, resize, delete; undo brings it back", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await tool(page, "tool.hole").click();
    await clickPlan(page, 8, 5);
    const ob = bar(page, "opening.hole");
    await expect(ob.locator(".which strong")).toHaveText(cs("opening.holeN", { n: 1 }));
    await setField(ob.getByLabel(cs("opening.width")), 1.5);
    await waitForProject(id, (p) => p.roof_openings?.length === 1);
    await areaMatchesCore(page, id);
    await ob.getByRole("button", { name: cs("opening.delete") }).click();
    await waitForProject(id, (p) => !p.roof_openings?.length);
    await tool(page, "editor.undo").click();
    await waitForProject(id, (p) => p.roof_openings?.length === 1);
  });

  test("a non-convex chimney stops the calculation and the notice shows it", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    const p = await loadProject(id);
    p.chimneys = [{ corners: [{ x: 5, y: 3 }, { x: 7, y: 3 }, { x: 6, y: 3.3 }, { x: 7, y: 5 }, { x: 5, y: 5 }], height: 9 }];
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Valba");
    const notice = page.locator(".warn-bar").filter({ hasText: cs("opening.nonConvexAll", { n: 1 }) });
    await expect(notice).toBeVisible();
    await notice.getByRole("button", { name: cs("opening.show") }).click();
    await expect(bar(page, "opening.chimney")).toContainText(cs("opening.nonConvex"));
    await page.keyboard.press("Delete");
    await expect(notice).toHaveCount(0);
    await expect(page.locator(".results .hero strong")).not.toHaveText("—");
  });
});

test.describe("hand-drawn lines and faces, flat roof", () => {
  test("roof line: two clicks, change type, length and label, delete", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await tool(page, "tool.line").click();
    await clickPlan(page, 2, 2);
    await expect(page.getByText(cs("plan.placeLineEnd"))).toBeVisible();
    await clickPlan(page, 8, 2);
    const mb = bar(page, "manual.line");
    await expect(mb.locator(".which strong")).toHaveText(cs("manual.lineN", { n: 1 }));
    let p = await waitForProject(id, (p) => p.manual_lines?.length === 1);
    expect(p.manual_lines[0].code).toBe("M");
    await mb.getByLabel(cs("manual.type")).selectOption("T");
    await setField(mb.getByLabel(cs("manual.length")), 7.25);
    await mb.getByLabel(cs("manual.label")).fill("Okap garáž");
    await mb.getByLabel(cs("manual.label")).press("Tab");
    p = await waitForProject(id, (p) => p.manual_lines[0].label === "Okap garáž");
    expect(p.manual_lines[0]).toMatchObject({ code: "T", length: 7.25 });
    await expect(mb).toContainText(cs("manual.counted"));
    await mb.getByRole("button", { name: cs("manual.measured") }).click();
    await waitForProject(id, (p) => p.manual_lines[0].length == null);
    await areaMatchesCore(page, id);
    await mb.getByRole("button", { name: cs("manual.delete") }).click();
    await waitForProject(id, (p) => !p.manual_lines?.length);
  });

  test("new face: corners, close on the first, pitch and label, delete with Delete", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const a0 = await shownArea(page);
    await tool(page, "tool.face").click();
    await clickPlan(page, 1, 1);
    await clickPlan(page, 4, 1);
    await clickPlan(page, 4, 3);
    await clickPlan(page, 1, 1);
    const mb = bar(page, "manual.face");
    await expect(mb.locator(".which strong")).toHaveText(cs("manual.faceN", { n: 1 }));
    let p = await waitForProject(id, (p) => p.manual_faces?.length === 1);
    expect(p.manual_faces[0]).toMatchObject({ pitch_deg: 30 });
    expect(p.manual_faces[0].points).toHaveLength(3);
    await setField(mb.getByLabel(cs("edge.pitch")), 45);
    p = await waitForProject(id, (p) => p.manual_faces[0].pitch_deg === 45);
    await areaMatchesCore(page, id);
    expect(await shownArea(page)).toBeGreaterThan(a0);
    // Delete works once the focus is out of the fields.
    await mb.locator(".which").click();
    await page.keyboard.press("Delete");
    await waitForProject(id, (p) => !p.manual_faces?.length);
  });

  test("flat roof toggles on and off", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await tool(page, "tool.flat").click();
    await expect(tool(page, "tool.flat")).toHaveClass(/\bon\b/);
    await waitForProject(id, (p) => p.flat_roof === true);
    await areaMatchesCore(page, id);
    // 12 × 8 m plus 0.5 m overhang all round.
    expect(await shownArea(page)).toBeCloseTo(13 * 9, 1);
    await tool(page, "tool.flat").click();
    await waitForProject(id, (p) => !p.flat_roof);
  });
});

test.describe("parts", () => {
  test("transform: move, rotate, mirror with a copy, offset a wall; cancel does nothing", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await expect(tool(page, "tool.transform")).toBeDisabled();
    await selectWall(page, 0);
    const dlg = page.getByRole("dialog", { name: cs("xf.title") });

    await tool(page, "tool.transform").click();
    await setField(dlg.getByLabel(cs("xf.dx")), 2);
    await setField(dlg.getByLabel(cs("xf.dy")), -1);
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    let p = await waitForProject(id, (p) => p.parts[0].edges[0].start.x === 2);
    expect(p.parts[0].edges[0].start).toEqual({ x: 2, y: -1 });

    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    await dlg.getByRole("radio", { name: cs("xf.rotate") }).click();
    await setField(dlg.getByLabel(cs("xf.angle")), 90);
    await setField(dlg.getByLabel(cs("xf.cx")), 2);
    await setField(dlg.getByLabel(cs("xf.cy")), -1);
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    p = await waitForProject(id, (p) => Math.abs(p.parts[0].edges[1].start.x - 2) < 1e-6);
    expect(p.parts[0].edges[1].start.y).toBeCloseTo(11, 6);

    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    await dlg.getByRole("radio", { name: cs("xf.mirror") }).click();
    await setField(dlg.getByLabel(cs("xf.axis")), 90);
    await setField(dlg.getByLabel(cs("xf.cx")), 5);
    await setField(dlg.getByLabel(cs("xf.copies")), 1);
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    p = await waitForProject(id, (p) => p.parts.length === 2);
    await areaMatchesCore(page, id);

    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    await dlg.getByRole("radio", { name: cs("xf.wall") }).click();
    await setField(dlg.getByLabel(cs("xf.dist")), 1);
    await dlg.getByRole("button", { name: cs("common.ok") }).click();
    await waitForProject(id, (p) => Math.abs(p.parts[0].edges[0].start.x - 3) < 1e-6);

    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    await setField(dlg.getByLabel(cs("xf.dx")), 50);
    await dlg.getByRole("button", { name: cs("common.cancel") }).click();
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts[0].edges[0].start.x).toBeCloseTo(3, 6);
  });

  test("delete part asks first; cancel keeps it", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    const p0 = await loadProject(id);
    const copy = structuredClone(p0.parts[0]);
    copy.name = "Garáž";
    for (const e of copy.edges) e.start.x += 15;
    p0.parts.push(copy);
    await bridge("save_project", { id, project: p0 });
    await openEditor(app, "Valba");
    await expect(tool(page, "tool.deletePart")).toBeDisabled();
    await selectWall(page, 4, 1);
    await app.answerConfirm(false);
    await tool(page, "tool.deletePart").click();
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts).toHaveLength(2);
    expect((await app.dialogs()).at(-1)?.message).toBe(cs("editor.confirmDeletePart", { name: "Garáž" }));
    await app.answerConfirm(true);
    await tool(page, "tool.deletePart").click();
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    expect(p.parts[0].name).not.toBe("Garáž");
    await areaMatchesCore(page, id);
  });
  test("clear asks first, empties the canvas, keeps the job and undoes", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    const p0 = await loadProject(id);
    p0.chimneys.push({ corners: [{ x: 4, y: 3 }, { x: 4.6, y: 3 }, { x: 4.6, y: 3.6 }, { x: 4, y: 3.6 }], height: 9 });
    p0.job.customer = "Novák";
    await bridge("save_project", { id, project: p0 });
    await openEditor(app, "Valba");
    await app.answerConfirm(false);
    await tool(page, "tool.clear").click();
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts).toHaveLength(1);
    expect((await app.dialogs()).at(-1)?.message).toBe(cs("editor.confirmClear"));
    await app.answerConfirm(true);
    await tool(page, "tool.clear").click();
    const p = await waitForProject(id, (p) => p.parts.length === 0);
    expect(p.chimneys).toHaveLength(0);
    expect(p.job.customer).toBe("Novák");
    await expect(tool(page, "tool.clear")).toBeDisabled();
    await expect(tool(page, "tool.draw")).toBeEnabled();
    await tool(page, "editor.undo").click();
    const back = await waitForProject(id, (p) => p.parts.length === 1);
    expect(back.chimneys).toHaveLength(1);
  });
});

test("3D view: every camera button and fit", async ({ app, page }) => {
  await createExample("l-shape", "Elko");
  await openEditor(app, "Elko");
  const views = page.getByRole("radiogroup", { name: cs("view.title") });
  for (const v of await views.getByRole("radio").all()) {
    await v.click();
    await expect(v).toHaveAttribute("aria-checked", "true");
  }
  await page.locator(".view").last().getByRole("button", { name: cs("plan.fit") }).click();
  await expect(page.locator(".view canvas")).toBeVisible();
});
