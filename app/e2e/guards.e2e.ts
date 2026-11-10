// Guards against losing or doubling work (review 2026-11-10): leaving waits
// for field handlers that ask the core (ERR-004), Accept / Revert run once
// (UI-002), Revert all asks (UX-011), a half-drawn footprint is not dropped
// silently (UX-013), no export or offer from a roof the core refused
// (ERR-003), the standard roof dialog adds the picked type (UI-001), browser
// ResizeObserver noise is no error (UI-019), Enter creates a new job (UX-017).
import type { Page, Route } from "@playwright/test";
import { bar, clickPlan, openEditor, selectWall, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

/** Holds every call of a bridge command for `ms` before it goes through. */
const slow = (page: Page, cmd: string, ms: number) =>
  page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    await new Promise((res) => setTimeout(res, ms));
    await r.continue();
  });

const back = (page: Page) => page.getByRole("button", { name: cs("editor.back") });
const libraryRow = (page: Page, name: string) => page.getByRole("row").filter({ hasText: name }).first();

test.describe("leaving waits for a value still with the core (ERR-004)", () => {
  test("Back right after a window size is typed stores the size before the library shows", async ({ app, page }) => {
    const id = await createExample("gable", "Okno");
    const p = await loadProject(id);
    p.windows = [{ x: 4.5, y: 1, width: 0.78, height: 1.18 }];
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Okno");
    await page.locator("path.opening.window").first().dispatchEvent("pointerdown", { button: 0 });
    const ob = bar(page, "opening.window");
    await slow(page, "check_windows", 800);
    const width = ob.getByLabel(cs("opening.width"));
    await width.click();
    await width.press("ControlOrMeta+a");
    await page.keyboard.type("1,14");
    await back(page).click();
    await expect(libraryRow(page, "Okno")).toBeVisible();
    // Read the moment the library shows: the editor is gone by now.
    expect((await loadProject(id)).windows[0].width).toBe(1.14);
  });

  test("Back right after the building height is typed stores it before the library shows", async ({ app, page }) => {
    const id = await createExample("gable", "Výška");
    await openEditor(app, "Výška");
    await selectWall(page, 0);
    await slow(page, "set_part_height", 800);
    const h = page.getByTestId("building-height");
    await h.click();
    await h.press("ControlOrMeta+a");
    await page.keyboard.type("4,2");
    await back(page).click();
    await expect(libraryRow(page, "Výška")).toBeVisible();
    const edges = (await loadProject(id)).parts[0].edges;
    expect(edges.every((e: any) => e.eave_height === 4.2)).toBe(true);
  });
});

const review = (page: Page) => page.locator(".review");
const pitches = async (id: string) => (await loadProject(id)).parts[0].edges.map((e: any) => e.pitch_deg).join(",");

test.describe("Accept and Revert run once (UI-002)", () => {
  test("a double-click on Revert in the AI tab reverts only the latest AI step", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Dvojklik");
    await openEditor(app, "Dvojklik");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 25 }], summary: "Krok A" });
    await expect(review(page)).toContainText("Krok A");
    await mcp.call("update_edges", { id, changes: [{ index: 2, pitch_deg: 26 }], summary: "Krok B" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.ai") }).click();
    await slow(page, "revert_last_change", 400);
    await page.locator("aside .panel").getByRole("button", { name: cs("ai.revert"), exact: true }).first().dblclick();
    await waitForProject(id, (p) => p.parts[0].edges[2].pitch_deg === 40);
    await page.waitForTimeout(1500);
    expect(await pitches(id)).toBe("25,90,40,90");
  });

  test("a double-click on Accept accepts once and leaves no error", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Přijmout");
    await openEditor(app, "Přijmout");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 25 }], summary: "Krok A" });
    await expect(review(page)).toBeVisible();
    await review(page).getByRole("button", { name: cs("review.accept") }).dblclick();
    await expect(review(page)).toHaveCount(0);
    expect(await pitches(id)).toBe("25,90,40,90");
    await expect(app.toasts()).toHaveCount(0);
  });
});

test.describe("Revert all asks first (UX-011)", () => {
  test("Revert all names the AI steps and keeps them when the user says no", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Vše");
    await openEditor(app, "Vše");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "Krok 1" });
    await expect(review(page)).toContainText("Krok 1");
    await mcp.call("update_edges", { id, changes: [{ index: 2, pitch_deg: 26 }], summary: "Krok 2" });
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Krok 3" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await app.answerConfirm(false);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toEqual([cs("review.confirmRevertAll", { n: 3 })]);
    await page.waitForTimeout(800);
    expect(await pitches(id)).toBe("30,90,26,90");
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await app.answerConfirm(true);
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 40 && p.parts[0].edges[2].pitch_deg === 40);
    expect(p.job.customer).toBe("");
    await expect(review(page)).toHaveCount(0);
  });

  test("Revert of a single AI step from the banner does not ask", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Jeden");
    await openEditor(app, "Jeden");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "Krok 1" });
    await expect(review(page)).toContainText("Krok 1");
    await review(page).getByRole("button", { name: cs("ai.revert"), exact: true }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 40);
    expect(await app.dialogs()).toEqual([]);
  });
});

test.describe("a half-drawn footprint is not dropped without asking (UX-013)", () => {
  async function drawThree(page: Page) {
    await tool(page, "tool.draw").click();
    const draw = bar(page, "draw.title");
    await clickPlan(page, 12.5, 1);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("3");
    await len.press("ArrowRight");
    await len.fill("4");
    await len.press("ArrowUp");
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    return draw;
  }

  test("another tool asks; No keeps drawing, Yes switches the tool", async ({ app, page }) => {
    await createExample("gable", "Kreslení");
    await openEditor(app, "Kreslení");
    const draw = await drawThree(page);
    await app.answerConfirm(false);
    await tool(page, "tool.chimney").click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toEqual([cs("draw.confirmCancel", { n: 3 })]);
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    await expect(tool(page, "tool.chimney")).toHaveAttribute("aria-pressed", "false");
    await app.answerConfirm(true);
    await tool(page, "tool.chimney").click();
    await expect(draw).toHaveCount(0);
    await expect(tool(page, "tool.chimney")).toHaveAttribute("aria-pressed", "true");
  });

  test("Back asks; No stays in the editor with the corners, Yes goes to the library", async ({ app, page }) => {
    await createExample("gable", "Odchod");
    await openEditor(app, "Odchod");
    const draw = await drawThree(page);
    await app.answerConfirm(false);
    await back(page).click();
    await expect.poll(async () => (await app.dialogs()).map((d) => d.message)).toEqual([cs("draw.confirmCancel", { n: 3 })]);
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    await app.answerConfirm(true);
    await back(page).click();
    await expect(libraryRow(page, "Odchod")).toBeVisible();
  });
});

test.describe("no export or offer from a roof the core refused (ERR-003)", () => {
  const nonConvex = async (name: string) => {
    const id = await createExample("hip", name);
    const p = await loadProject(id);
    p.chimneys = [{ corners: [{ x: 5, y: 3 }, { x: 7, y: 3 }, { x: 6, y: 3.3 }, { x: 7, y: 5 }, { x: 5, y: 5 }], height: 9 }];
    await bridge("save_project", { id, project: p });
    return id;
  };
  const jobTab = (page: Page) => page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") });

  test("Export is disabled with the reason while the roof has a calculation error", async ({ app, page }) => {
    await nonConvex("Chyba");
    await openEditor(app, "Chyba");
    await expect(page.locator(".warn-bar").filter({ hasText: cs("opening.nonConvexAll", { n: 1 }) })).toBeVisible();
    const exp = tool(page, "tool.export");
    await expect(exp).toBeDisabled();
    await expect(exp).toHaveAttribute("title", cs("tool.exportBlocked"));
    // Fixed: export works again.
    await page.locator(".warn-bar").getByRole("button", { name: cs("opening.show") }).click();
    await page.keyboard.press("Delete");
    await expect(exp).toBeEnabled();
    await expect(exp).toHaveAttribute("title", cs("tool.exportHint"));
  });

  test("the job cannot be set to offer or ordered while the roof has a calculation error", async ({ app, page }) => {
    const id = await nonConvex("Nabídka");
    await openEditor(app, "Nabídka");
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    await expect(status).toHaveAttribute("title", cs("tool.exportBlocked"));
    // (Playwright's toBeDisabled does not look at <option>.)
    await expect(status.locator('option[value="offer"]')).toHaveAttribute("disabled", "");
    await expect(status.locator('option[value="ordered"]')).toHaveAttribute("disabled", "");
    await expect(status.locator('option[value="draft"]')).not.toHaveAttribute("disabled");
    await page.waitForTimeout(700);
    const p = await loadProject(id);
    expect(p.job.status).toBe("draft");
    expect(p.result_snapshot ?? null).toBeNull();
  });

  test("the stored result cannot be replaced by the figures of a roof the core refused (DR-001)", async ({ app, page }) => {
    const id = await createExample("hip", "Uložená");
    let p = await loadProject(id);
    p.job.status = "offer";
    p.result_snapshot = await bridge("take_snapshot", { project: p });
    await bridge("save_project", { id, project: p });
    p = await loadProject(id);
    p.chimneys = [{ corners: [{ x: 5, y: 3 }, { x: 7, y: 3 }, { x: 6, y: 3.3 }, { x: 7, y: 5 }, { x: 5, y: 5 }], height: 9 }];
    await bridge("save_project", { id, project: p });
    const area = p.result_snapshot.roof_area;
    expect(area).toBeGreaterThan(0);
    await openEditor(app, "Uložená");
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.results") }).click();
    const store = page.locator(".results").getByRole("button", { name: cs("snap.store") });
    await expect(store).toBeVisible();
    await expect(store).toBeDisabled();
    await expect(store).toHaveAttribute("title", cs("tool.exportBlocked"));
    await page.waitForTimeout(700);
    expect((await loadProject(id)).result_snapshot.roof_area).toBe(area);
  });

  const failCmd = (page: Page, cmd: string, code: string) =>
    page.route(new RegExp(`/${cmd}$`), (r) =>
      r.fulfill({ contentType: "application/json", headers: { "access-control-allow-origin": "*" }, body: JSON.stringify({ err: { code, detail: "too many pitch breaks of one wall: 65 (at most 64)" } }) }),
    );

  test("a calculation that fails as a whole blocks export, offer and order too (DR-002)", async ({ app, page }) => {
    const id = await createExample("hip", "Bez výsledku");
    await failCmd(page, "calc", "too_many");
    await openEditor(app, "Bez výsledku");
    await expect(app.toasts().filter({ hasText: cs("err.too_many") })).toBeVisible();
    const exp = tool(page, "tool.export");
    await expect(exp).toBeDisabled();
    await expect(exp).toHaveAttribute("title", cs("tool.exportBlocked"));
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    await expect(status.locator('option[value="offer"]')).toHaveAttribute("disabled", "");
    await expect(status.locator('option[value="ordered"]')).toHaveAttribute("disabled", "");
    expect((await loadProject(id)).job.status).toBe("draft");
    // The failed calculation is reported on purpose.
    app.errors.length = 0;
  });

  test("the job stays a draft when its result cannot be stored (DR-002)", async ({ app, page }) => {
    const id = await createExample("hip", "Bez snímku");
    await openEditor(app, "Bez snímku");
    await failCmd(page, "take_snapshot", "not_calculated");
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    await status.selectOption("offer");
    await expect(app.toasts()).toHaveCount(1);
    await expect(status).toHaveValue("draft");
    await page.waitForTimeout(800);
    const p = await loadProject(id);
    expect(p.job.status).toBe("draft");
    expect(p.result_snapshot ?? null).toBeNull();
    app.errors.length = 0;
  });

  test("a roof with only a warning still exports and can be offered", async ({ app, page }) => {
    const id = await createExample("gable", "Varování");
    const p = await loadProject(id);
    // A window off the roof: a warning, the roof is still calculated.
    p.windows = [{ x: 40, y: 40, width: 0.78, height: 1.18 }];
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Varování");
    await expect(page.locator(".results")).toContainText(cs("res.warn.title"));
    await expect(tool(page, "tool.export")).toBeEnabled();
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    await expect(status.locator('option[value="offer"]')).not.toHaveAttribute("disabled");
    await status.selectOption("offer");
    await expect.poll(async () => (await loadProject(id)).job.status).toBe("offer");
  });
});

test.describe("the standard roof dialog adds the type picked (UI-001)", () => {
  test("OK waits for the preview of the picked type and adds that type", async ({ app, page }) => {
    const id = await createExample("gable", "Standard");
    await openEditor(app, "Standard");
    await slow(page, "standard_part", 900);
    await tool(page, "tool.standard").click();
    const dlg = page.getByRole("dialog", { name: cs("std.title") });
    const ok = dlg.getByRole("button", { name: cs("common.ok") });
    await expect(ok).toBeEnabled({ timeout: 8000 });
    await dlg.getByRole("radio", { name: cs("std.hip"), exact: true }).click();
    await expect(ok).toBeDisabled();
    await dlg.getByLabel(cs("std.add")).check();
    await expect(ok).toBeEnabled({ timeout: 8000 });
    await ok.click();
    const p = await waitForProject(id, (p) => p.parts.length === 2);
    // A hip roof: every wall an eave (a gable wall would be at 90°).
    expect(p.parts[1].edges.every((e: any) => e.pitch_deg !== 90)).toBe(true);
    await expect(app.toasts()).toHaveCount(0);
  });

  test("closing the dialog while the preview is calculated leaves no error", async ({ app, page }) => {
    const id = await createExample("gable", "Zavřít");
    await openEditor(app, "Zavřít");
    await slow(page, "standard_part", 900);
    await tool(page, "tool.standard").click();
    const dlg = page.getByRole("dialog", { name: cs("std.title") });
    await dlg.getByRole("radio", { name: cs("std.pyramid"), exact: true }).click();
    await dlg.getByRole("button", { name: cs("common.cancel") }).click();
    await expect(dlg).toHaveCount(0);
    await page.waitForTimeout(1500);
    expect((await loadProject(id)).parts).toHaveLength(1);
    await expect(app.toasts()).toHaveCount(0);
  });
});

test.describe("browser noise is not an error (UI-019)", () => {
  test("a ResizeObserver loop message shows no toast; a real error still does", async ({ app, page }) => {
    await createExample("gable", "Okno prohlížeče");
    await openEditor(app, "Okno prohlížeče");
    await page.evaluate(() => window.dispatchEvent(new ErrorEvent("error", { message: "ResizeObserver loop completed with undelivered notifications." })));
    await page.evaluate(() => window.dispatchEvent(new ErrorEvent("error", { message: "ResizeObserver loop limit exceeded" })));
    await page.waitForTimeout(400);
    await expect(app.toasts()).toHaveCount(0);
    await page.evaluate(() => window.dispatchEvent(new ErrorEvent("error", { message: "boom", error: new Error("boom") })));
    await expect(app.toasts()).toHaveCount(1);
    // The real error above is logged on purpose.
    app.errors.length = 0;
  });
});

test.describe("Enter creates the new job (UX-017)", () => {
  test("Enter in the name field creates the job once Create is enabled", async ({ app, page }) => {
    await app.open();
    await slow(page, "standard_part", 900);
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    const name = dlg.getByLabel(cs("job.name"));
    const create = dlg.getByRole("button", { name: cs("new.create") });
    await name.fill("Enterem");
    // The standard roof preview is still on its way: Create is off, so is Enter.
    await expect(create).toBeDisabled();
    await name.press("Enter");
    await expect(dlg).toBeVisible();
    expect(await listProjects()).toHaveLength(0);
    await expect(create).toBeEnabled({ timeout: 8000 });
    await name.press("Enter");
    await expect(page.locator(".editor .statusbar")).toBeVisible();
    await expect(dlg).toHaveCount(0);
    const jobs = await listProjects();
    expect(jobs.map((j: any) => j.name)).toEqual(["Enterem"]);
    expect((await loadProject(jobs[0].id)).parts).toHaveLength(1);
  });
});
