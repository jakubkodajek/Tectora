// Editor and library feedback found in the 2026-11-08 review: the Delete key
// on a wall asks first, the selected wall stands out from an eave, the
// calculation breakdown shows no float noise, a manual face is in the 3D
// scene, and a double click on Duplicate / Delete in the library acts once.
import { bar, clickPlan, openEditor, selectWall, setField, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

test.describe("editor feedback", () => {
  test("Delete or Backspace with a wall selected asks before removing a corner (UX-006)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    // Focus on the Next button, as after stepping with ◀ ▶.
    await def.getByRole("button", { name: cs("def.next") }).focus();
    await app.answerConfirm(false);
    await page.keyboard.press("Backspace");
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toContain(cs("edge.confirmDeleteVertex", { n: 1, name: cs("editor.partN", { n: 1 }) }));
    await page.waitForTimeout(700);
    expect((await loadProject(id)).parts[0].edges).toHaveLength(4);
    await app.answerConfirm(true);
    await page.keyboard.press("Delete");
    await waitForProject(id, (p) => p.parts[0].edges.length === 3);
  });

  test("the selected wall does not look like an unselected eave (UX-007)", async ({ app, page }) => {
    await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await selectWall(page, 0);
    const look = (i: number) =>
      page.locator(".view .wrap > svg g.edge").nth(i).evaluate((g) => {
        const w = getComputedStyle(g.querySelector(".wall")!);
        const halo = g.querySelector(".sel-halo");
        return { stroke: w.stroke, halo: halo ? parseFloat(getComputedStyle(halo).strokeWidth) : 0 };
      });
    const [sel, other] = [await look(0), await look(2)];
    expect(sel.stroke).not.toBe(other.stroke);
    expect(sel.halo).toBeGreaterThanOrEqual(10);
    expect(other.halo).toBe(0);
  });

  test("the calculation breakdown shows plan areas without float noise (UX-014)", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await page.getByRole("tab", { name: cs("dock.breakdown") }).click();
    const text = (await page.locator("aside").textContent()) ?? "";
    expect(text).toContain("÷ cos");
    // e.g. 58,499997 for 13 × 4,5: no run of 5+ nines or zeros after the comma.
    expect(text).not.toMatch(/\d,\d*(9{5,}|0{5,})\d* m²/);
  });

  test("a manual face is drawn in 3D and can be picked there (UX-009)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const p = await loadProject(id);
    p.manual_faces = [{ points: [{ x: 16, y: 0 }, { x: 20, y: 0 }, { x: 20, y: 4 }, { x: 16, y: 4 }], pitch_deg: 30 }];
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Sedlo");
    // The 3D view's framing covers the manual face, so it is in the scene.
    await expect
      .poll(async () => Number(((await page.locator(".host[data-frame]").getAttribute("data-frame")) ?? "").split(",")[2]))
      .toBeGreaterThan(19.9);
  });
});

test.describe("library", () => {
  test("a double click on Duplicate makes one copy, on Delete asks once (UI-018)", async ({ app, page }) => {
    await createExample("hip", "Valba");
    await app.open();
    await page.getByRole("row").filter({ hasText: "Valba" }).click();
    const detail = page.locator("aside.detail");
    await detail.getByRole("button", { name: cs("card.duplicate") }).dblclick();
    await expect.poll(async () => (await listProjects()).length).toBe(2);
    await page.waitForTimeout(800);
    expect(await listProjects()).toHaveLength(2);
    await expect(detail.getByRole("heading")).toHaveText(cs("lib.copyName", { name: "Valba" }));
    await app.answerConfirm(true, true);
    await detail.getByRole("button", { name: cs("card.delete") }).dblclick();
    await expect.poll(async () => (await listProjects()).length).toBe(1);
    await page.waitForTimeout(800);
    expect((await app.dialogs()).filter((d) => d.kind === "confirm")).toHaveLength(1);
    await expect(page.locator(".toast")).toHaveCount(0);
  });
});

// Audit 2026-11-24 UX-1902 (owner decision 3): Ctrl+Z while the corners of a
// new building or a hand-drawn face are placed removes only the last corner;
// the roof edit made before stays. Backspace does the same for a face.
test.describe("corners being placed (UX-1902)", () => {
  for (const kind of ["walls", "face"] as const) {
    test(`Ctrl+Z while placing ${kind === "walls" ? "a building's" : "a face's"} corners removes only the last corner (UX-1902)`, async ({ app, page }) => {
      const id = await createExample("gable", "R20 Ctrl Z");
      await openEditor(app, "R20 Ctrl Z");
      await app.saved();
      await selectWall(page, 0);
      await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 45);
      await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 45);
      await app.saved();
      await tool(page, kind === "walls" ? "tool.draw" : "tool.face").click();
      await clickPlan(page, 2, 1);
      await clickPlan(page, 6, 1);
      const count = kind === "walls" ? page.locator(".bar").filter({ hasText: cs("draw.points", { n: 2 }) }) : page.getByTestId("place-points");
      if (kind === "walls") await expect(count).toHaveCount(1);
      else await expect(count).toHaveText(cs("place.points", { n: 2 }));
      await page.locator("body").press("ControlOrMeta+z");
      if (kind === "walls") await expect(page.locator(".bar").filter({ hasText: cs("draw.points", { n: 1 }) })).toHaveCount(1);
      else await expect(count).toHaveText(cs("place.points", { n: 1 }));
      // Redo does nothing while corners are placed.
      await page.locator("body").press("ControlOrMeta+Shift+z");
      await page.waitForTimeout(700);
      expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(45);
      if (kind === "face") {
        await clickPlan(page, 6, 3);
        await expect(count).toHaveText(cs("place.points", { n: 2 }));
        await page.locator("body").press("Backspace");
        await expect(count).toHaveText(cs("place.points", { n: 1 }));
        expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(45);
      }
    });
  }
});

// Audit 2026-11-24 UX-2002 (round 21, owner decision 6): with no corner left,
// Ctrl+Z, Backspace and Delete do nothing while corners are placed, and
// picking a drawing tool ends the wall selection, so one press more than
// there are corners never undoes the earlier edit or deletes a wall.
// TEST-2009 U19 / U19b: a redo step is waiting, so Redo while corners are
// placed would show (a corner gone, or the pitch back to 50).
test.describe("one press more than there are corners (UX-2002)", () => {
  const pitchField = (page: import("@playwright/test").Page) => bar(page, "def.title").getByLabel(cs("edge.pitch"));
  for (const kind of ["walls", "face"] as const) {
    test(`placing ${kind === "walls" ? "a building's" : "a face's"} corners: an extra Ctrl+Z or Redo keeps the roof edit, the corners count right (UX-2002, TEST-2009)`, async ({ app, page }) => {
      const id = await createExample("gable", "R21 Ctrl Z");
      await openEditor(app, "R21 Ctrl Z");
      await app.saved();
      await selectWall(page, 0);
      await setField(pitchField(page), 45);
      await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 45);
      await setField(pitchField(page), 50);
      await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 50);
      // One Undo before placing: a Redo step waits (pitch 50).
      await page.locator("body").press("ControlOrMeta+z");
      await expect(pitchField(page)).toHaveValue("45");
      await app.saved();
      await tool(page, kind === "walls" ? "tool.draw" : "tool.face").click();
      // Picking the tool ends the wall selection (decision 6).
      await expect(pitchField(page)).toHaveCount(0);
      await clickPlan(page, 2, 1);
      await clickPlan(page, 6, 1);
      const shows = (n: number) =>
        kind === "walls"
          ? expect(page.locator(".bar").filter({ hasText: n === 0 ? cs("draw.start") : cs("draw.points", { n }) })).toHaveCount(1)
          : expect(page.getByTestId("place-points")).toHaveText(cs("place.points", { n }));
      await shows(2);
      // Redo with two corners: nothing (U19 dropped a corner, U19b redid the pitch).
      await page.locator("body").press("ControlOrMeta+Shift+z");
      await shows(2);
      await page.locator("body").press("ControlOrMeta+z");
      await shows(1);
      await page.locator("body").press("ControlOrMeta+z");
      await shows(0);
      const leave = async () => {
        if (kind === "walls") await page.locator(".bar").getByRole("button", { name: cs("draw.cancel") }).click();
        else await page.locator("body").press("Escape");
        await selectWall(page, 0);
      };
      // The press more: nothing at all (it undid the pitch to 40).
      await page.locator("body").press("ControlOrMeta+z");
      await shows(0);
      // A corner placed after it is the first one: the tool is still on.
      await clickPlan(page, 2, 1);
      await shows(1);
      // Leave the tool and look at the wall: still 45.
      await leave();
      await expect(pitchField(page)).toHaveValue("45");
      // Redo with no corner placed: nothing either (it redid the pitch to 50).
      await tool(page, kind === "walls" ? "tool.draw" : "tool.face").click();
      await shows(0);
      await page.locator("body").press("ControlOrMeta+Shift+z");
      await clickPlan(page, 2, 1);
      await shows(1);
      await leave();
      await expect(pitchField(page)).toHaveValue("45");
      await app.saved();
      const saved = await loadProject(id);
      expect(saved.parts[0].edges[0].pitch_deg).toBe(45);
      expect(saved.parts).toHaveLength(1);
      expect(saved.parts[0].edges).toHaveLength(4);
    });
  }

  test("Delete and Backspace while placing a face's corners never delete the wall selected before (UX-2002)", async ({ app, page }) => {
    const id = await createExample("gable", "R21 Delete");
    await openEditor(app, "R21 Delete");
    await app.saved();
    await selectWall(page, 0);
    await expect(pitchField(page)).toHaveCount(1);
    await tool(page, "tool.face").click();
    await expect(pitchField(page)).toHaveCount(0);
    const count = page.getByTestId("place-points");
    await clickPlan(page, 2, 1);
    await clickPlan(page, 6, 1);
    await expect(count).toHaveText(cs("place.points", { n: 2 }));
    // Delete with two corners placed: no question, the corners stay.
    await page.locator("body").press("Delete");
    await expect(count).toHaveText(cs("place.points", { n: 2 }));
    // Backspace removes corners, and one more does nothing.
    for (const n of [1, 0, 0]) {
      await page.locator("body").press("Backspace");
      await expect(count).toHaveText(cs("place.points", { n }));
    }
    await page.locator("body").press("Delete");
    // A corner placed after it shows the tool and its keys were all handled.
    await clickPlan(page, 2, 1);
    await expect(count).toHaveText(cs("place.points", { n: 1 }));
    expect(await app.dialogs()).toEqual([]);
    await app.saved();
    expect((await loadProject(id)).parts[0].edges).toHaveLength(4);
  });
});
