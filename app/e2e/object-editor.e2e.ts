// The chimney / roof window / opening editor in named groups (Jakub
// 2026-11-28: "terribly unclear"): the bar's groups and More row, the
// Properties dialog with its sketch of the roof face, and delete in the
// dialog's footer. The gable example is 12 x 8 m, 40°, 0.5 m overhang.
import { bar, clickPlan, openEditor } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const chimney = (height: number) => ({ corners: [{ x: 8, y: 5.5 }, { x: 8.6, y: 5.5 }, { x: 8.6, y: 6.1 }, { x: 8, y: 6.1 }], height });

async function withChimney(name: string, height: number) {
  const id = await createExample("gable", name);
  const p = await loadProject(id);
  p.chimneys = [chimney(height)];
  await bridge("save_project", { id, project: p });
  return id;
}

test.describe("object editor", () => {
  test("the bar shows size and position on the roof; More adds position in plan and the facts, and stays open", async ({ app, page }) => {
    await withChimney("Skupiny", 6.5);
    await openEditor(app, "Skupiny");
    await clickPlan(page, 8.3, 5.8);
    const ob = bar(page, "opening.chimney");
    await expect(ob.getByRole("group", { name: cs("opening.size") })).toBeVisible();
    await expect(ob.getByRole("group", { name: cs("place.title") })).toBeVisible();
    await expect(ob.getByRole("group", { name: cs("place.plan") })).toHaveCount(0);
    const more = ob.getByTestId("object-more");
    await expect(more).toHaveAttribute("aria-expanded", "false");
    await more.click();
    await expect(ob.getByRole("group", { name: cs("place.plan") })).toBeVisible();
    await expect(ob.getByLabel(cs("place.cornerX"))).toHaveValue("8");
    // 0.36 m² in plan on 40° is the 0.47 m² the results panel deducts.
    await expect(ob.getByTestId("object-area")).toHaveText(cs("opening.slopeArea", { a: "0,47", p: "0,36" }));
    await expect(ob.getByTestId("object-face")).toHaveText(cs("place.face", { part: cs("editor.partN", { n: 1 }), n: 3, p: "40" }));
    await expect(ob.getByTestId("object-above")).toBeVisible();
    // The choice is the user's: the next object opens with More open too.
    await page.keyboard.press("Escape");
    await clickPlan(page, 8.3, 5.8);
    await expect(bar(page, "opening.chimney").getByTestId("object-more")).toHaveAttribute("aria-expanded", "true");
  });

  test("a chimney whose top is under the roof says so", async ({ app, page }) => {
    await withChimney("Nízký komín", 3);
    await openEditor(app, "Nízký komín");
    await clickPlan(page, 8.3, 5.8);
    await expect(bar(page, "opening.chimney").getByTestId("object-top-below")).toHaveText(cs("opening.topBelow"));
  });

  test("Properties opens the dialog; its sketch shows the three distances of the fields; Delete in the footer deletes", async ({ app, page }) => {
    const id = await withChimney("Vlastnosti", 6.5);
    await openEditor(app, "Vlastnosti");
    await clickPlan(page, 8.3, 5.8);
    await bar(page, "opening.chimney").getByTestId("object-properties").click();
    const dlg = page.getByTestId("object-dialog");
    await expect(dlg).toBeVisible();
    const sketch = dlg.getByTestId("place-sketch");
    for (const [key, testid] of [["eave", "place-eave"], ["left", "place-left"], ["right", "place-right"]]) {
      const v = await dlg.getByTestId(testid).inputValue();
      await expect(sketch.locator(`[data-dim="${key}"] text`)).toHaveText(v);
    }
    await expect(sketch.getByTestId("place-sketch-base")).toHaveText(cs("place.eaveMark"));
    // The focused distance is the one drawn solid.
    await dlg.getByTestId("place-left").focus();
    await expect(sketch.locator('[data-dim="left"]')).toHaveClass(/\bon\b/);
    await expect(sketch.locator('[data-dim="eave"]')).toHaveClass(/\boff\b/);
    await page.getByTestId("object-dialog-delete").click();
    await expect(dlg).toHaveCount(0);
    await waitForProject(id, (p) => p.chimneys.length === 0);
  });
});
