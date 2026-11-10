// Editor and library feedback found in the 2026-11-08 review: the Delete key
// on a wall asks first, the selected wall stands out from an eave, the
// calculation breakdown shows no float noise, a manual face is in the 3D
// scene, and a double click on Duplicate / Delete in the library acts once.
import { bar, openEditor, selectWall } from "./editor";
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
