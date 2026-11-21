// Guards for the round-2 audit (2026-11-21) dormer findings: a value typed
// in the dormer bar lands on its dormer when ▶ or PageDown moves on
// (UI-101), and the dormer dialog's OK waits for the core's check and stays
// open on a refusal or a front off the roof (UI-102, UX-109).
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, tool } from "./editor";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

/** Types like a person: click, select all, type (no fill()). */
async function typeInto(page: Page, field: import("@playwright/test").Locator, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

const width = (d: any) => +Math.hypot(d.end.x - d.edges[0].start.x, d.end.y - d.edges[0].start.y).toFixed(3);

test.describe("round-2 audit: dormers", () => {
  test("a width typed in the dormer bar stays on its dormer when ▶ or PageDown moves on (UI-101)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Dva vikýře");
    await openEditor(app, "Dva vikýře");
    await mcp.call("add_dormer", { id, kind: "gable", start: [2, 1], summary: "Dormer 1." });
    await mcp.call("add_dormer", { id, kind: "gable", start: [8, 1], summary: "Dormer 2." });
    await expect(page.locator("g.dormer")).toHaveCount(2);
    // Accept the AI's dormers, so the review banner does not move the plan.
    await page.getByRole("button", { name: cs("review.accept") }).first().click();
    const db = bar(page, "dormer.title");
    const first = async () => {
      const bb = (await page.locator("g.dormer").nth(0).locator("path.dormer-area").first().boundingBox())!;
      await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
      await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 1 }));
    };
    const other = width((await loadProject(id)).dormers[1]);

    await first();
    await typeInto(page, db.getByLabel(cs("dormer.width")), "3,1");
    await db.getByRole("button", { name: cs("dormer.next") }).click();
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 2 }));
    await waitForProject(id, (p) => width(p.dormers[0]) === 3.1);
    expect(width((await loadProject(id)).dormers[1])).toBe(other);

    await page.keyboard.press("Escape");
    await first();
    await typeInto(page, db.getByLabel(cs("dormer.width")), "2,7");
    await page.keyboard.press("PageDown");
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 2 }));
    await waitForProject(id, (p) => width(p.dormers[0]) === 2.7);
    expect(width((await loadProject(id)).dormers[1])).toBe(other);
  });

  const newDormer = async (app: import("./fixtures").App, page: Page, name: string) => {
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await tool(page, "tool.dormer").click();
    // On the lower face, the ridge at y = 4.
    await clickPlan(page, 5, 2.4);
    const dlg = page.getByRole("dialog", { name: cs("dormer.dialogNew") });
    await expect(dlg).toBeVisible();
    await page.waitForTimeout(800);
    return { id, dlg, ok: dlg.getByRole("button", { name: cs("common.ok"), exact: true }) };
  };

  test("the dormer dialog's OK right after typing a pitch stores the typed pitch (UI-102)", async ({ app, page }) => {
    const { id, dlg, ok } = await newDormer(app, page, "Sklon vikýře");
    await typeInto(page, dlg.getByLabel(cs("edge.pitch"), { exact: true }), "36");
    await ok.click();
    await expect(dlg).toBeHidden();
    const p = await waitForProject(id, (p) => p.dormers.length === 1);
    expect(p.dormers[0].edges.some((e: any) => e.pitch_deg === 36)).toBe(true);
  });

  test("a refused eave typed just before OK keeps the dialog open with the reason (UI-102)", async ({ app, page }) => {
    const { id, dlg, ok } = await newDormer(app, page, "Nízký okap");
    await typeInto(page, dlg.getByLabel(cs("dormer.eaveAbs")), "1");
    await ok.click();
    await expect(dlg.getByTestId("dormer-refusal")).toBeVisible();
    await expect(page.getByTestId("dormer-ok-blocked")).toBeVisible();
    await expect(dlg).toBeVisible();
    await expect(ok).toBeDisabled();
    await page.waitForTimeout(500);
    expect((await loadProject(id)).dormers).toHaveLength(0);
    app.errors.length = 0;
  });

  test("the dormer dialog's OK does not store a dormer whose front is off the roof (UX-109)", async ({ app, page }) => {
    const { id, dlg, ok } = await newDormer(app, page, "Široký vikýř");
    await typeInto(page, dlg.getByLabel(cs("dormer.width")), "70");
    await ok.click();
    await expect(page.getByTestId("dormer-ok-blocked")).toHaveText(cs("dormer.offRoof"));
    await expect(dlg).toBeVisible();
    await expect(ok).toBeDisabled();
    expect((await loadProject(id)).dormers).toHaveLength(0);
    // Back on the roof: OK stores it.
    await typeInto(page, dlg.getByLabel(cs("dormer.width")), "2");
    await page.keyboard.press("Tab");
    await expect(ok).toBeEnabled();
    await ok.click();
    await waitForProject(id, (p) => p.dormers.length === 1);
  });

});
