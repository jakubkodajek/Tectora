// Guards for the round-2 audit (2026-11-21) UI findings on the plan: a
// double click on Close face adds the face and keeps it (UI-103), and the
// pitch Eave brings back belongs to the job it was remembered in (UI-104),
// and Results says why a job has no result (UX-101).
import { bar, clickPlan, openEditor, selectWall, setField, tool } from "./editor";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

test.describe("round-2 audit: plan", () => {
  test("a double click on Close face adds the face and does not delete it (UI-103)", async ({ app, page }) => {
    const id = await createExample("gable", "Ruční dvojklik");
    await openEditor(app, "Ruční dvojklik");
    await tool(page, "tool.face").click();
    const pb = page.getByTestId("place-bar");
    for (const [x, y] of [
      [2, 1],
      [6, 1],
      [6, 3],
    ])
      await clickPlan(page, x, y);
    await pb.getByRole("button", { name: cs("place.closeFace") }).dblclick();
    await waitForProject(id, (p) => (p.manual_faces ?? []).length === 1);
    await page.waitForTimeout(800);
    expect((await loadProject(id)).manual_faces ?? []).toHaveLength(1);
  });

  test("Eave on a gable does not bring back a pitch remembered in another job (UI-104)", async ({ app, page }) => {
    const a = await createExample("hip", "Paměť A");
    // A gable roof from the same standard size: its wall 2 is a gable from the start.
    const b = await createExample("gable", "Paměť B");
    await openEditor(app, "Paměť A");
    const def = bar(page, "def.title");
    const type = (key: string) => def.getByRole("radio", { name: cs(key) }).click();
    await selectWall(page, 1);
    await setField(def.getByLabel(cs("edge.pitch")), "50");
    await waitForProject(a, (p) => p.parts[0].edges[1].pitch_deg === 50);
    await type("edge.gable");
    await waitForProject(a, (p) => p.parts[0].edges[1].pitch_deg === 90);
    await app.saved();
    // Back to the library in the same session, then job B (same corners).
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await app.openJob("Paměť B");
    const start = (await loadProject(b)).parts[0].edges;
    expect(start[1].pitch_deg).toBe(90);
    const neighbour = start[0].pitch_deg;
    await selectWall(page, 1);
    // B's wall 2 was never switched in this job: the neighbour's pitch, never A's 50°.
    await type("edge.eave");
    const p = await waitForProject(b, (p) => p.parts[0].edges[1].pitch_deg !== 90);
    expect(p.parts[0].edges[1].pitch_deg).toBe(neighbour);
    expect(p.parts[0].edges[1].pitch_deg).not.toBe(50);
  });

  test("a job the calculation refuses says why in Results, not that it has no building (UX-101)", async ({ app, page }) => {
    await createExample("hip", "Složitá");
    await page.route(/\/calc$/, (r) =>
      r.fulfill({ contentType: "application/json", headers: { "access-control-allow-origin": "*" }, body: JSON.stringify({ err: { code: "too_complex", detail: "too complex" } }) }),
    );
    await openEditor(app, "Složitá");
    const results = page.locator(".results");
    await expect(results.getByTestId("res-calc-failed")).toContainText("too_complex");
    await expect(results.getByTestId("res-calc-failed")).toContainText(cs("err.too_complex"));
    await expect(results.getByTestId("res-empty")).toHaveCount(0);
    await expect(results.getByTestId("res-area")).toHaveText("—");
    app.errors.length = 0;
  });
});
