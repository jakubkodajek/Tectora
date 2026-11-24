// Guards for the round-16 audit (2026-11-23), part B (outside saves, notices).
//
// ERR-1502  a save or a banner Revert that writes over a file another program
//           made unreadable says "replaced" from the store's answer, also when
//           the reload had not failed first
// UX-1502   that notice has a title of its own (the job stays open)
// UX-1504   it stays until something happens, and the status bar does not say
//           saved meanwhile
// TEST-1504 a good file the other program writes back clears it: the next
//           edit says nothing about a replaced file
// UI-1503   refusals of a newer Tectora's file name the job, also after Back
// HUNT-1502 an object off the roof is said while another building fails
// UX-1503   Results counts it apart as off the roof
// UI-1502   two placements before one calculation answers: each is checked,
//           and Undo takes the warning back
// TEST-1506 the off-roof warning for a roof opening; the library card quotes
//           an AI's summary
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, selectWall, setField, tool } from "./editor";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, loadProject, noToast, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r16";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r16-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

/** `{"a": {"a": … {}}}` with `n` objects inside each other. */
function nestedObjects(n: number): unknown {
  let v: unknown = {};
  for (let k = 1; k < n; k++) v = { a: v };
  return v;
}

/** Another program writes the job 40 levels deep, with its own notes. */
function writeTooDeep(file: string) {
  const outside = JSON.parse(fs.readFileSync(file, "utf8"));
  outside.crm = nestedObjects(40);
  outside.notes = "from CRM";
  fs.writeFileSync(file, JSON.stringify(outside));
}

/** The editor's reads of the job reach the core `ms` later (a slow disk or a busy core). */
async function slowReads(page: Page, ms: number) {
  await page.route("**/load_project", async (route) => {
    await new Promise((r) => setTimeout(r, ms));
    await route.continue().catch(() => {});
  });
}

const replaced = (app: { toasts(): import("@playwright/test").Locator }) => app.toasts().filter({ hasText: cs("editor.outsideReplaced") });
const notice = (app: { toasts(): import("@playwright/test").Locator }) => app.toasts().filter({ hasText: cs("editor.outsideUnreadable") });

test.describe("round 16, part B", () => {
  test("an edit saved over a file another program made unreadable before the reload says replaced, once (ERR-1502)", async ({ app, page }) => {
    const id = await createExample("hip", "R16 souběh");
    await openEditor(app, "R16 souběh");
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    // The reload of the outside write comes after the editor's save.
    await slowReads(page, 2500);
    writeTooDeep(file);
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 31);
    await expect(replaced(app)).toHaveCount(1);
    await app.saved();
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    expect([p.parts[0].edges[0].pitch_deg, "crm" in p]).toEqual([31, false]);
    // The late reload finds the editor's own file: no warning about a save to come.
    await page.waitForTimeout(3000);
    await expect(notice(app)).toHaveCount(0);
    await expect(replaced(app)).toHaveCount(1);
  });

  test("the outside-unreadable notice has its own title, stays, and the status bar does not say saved (UX-1502, UX-1504)", async ({ app, page }) => {
    const id = await createExample("hip", "R16 cizí");
    await openEditor(app, "R16 cizí");
    await app.saved();
    writeTooDeep(path.join(LIBRARY_DIR, `${id}.tectora`));
    await expect(notice(app)).toHaveCount(1);
    await expect(notice(app)).toContainText(cs("err.ofJob", { name: "R16 cizí", title: cs("editor.outsideTitle") }));
    await expect(notice(app)).not.toContainText(cs("err.action.load"));
    // Longer than any toast's timer.
    await page.waitForTimeout(12_500);
    await expect(notice(app)).toHaveCount(1);
    await expect(page.locator(".statusbar .save")).toHaveText(cs("editor.outsideState"));
    await expect(page.locator(".statusbar .save")).not.toContainText(cs("editor.saved"));
    if (process.env.TECTORA_SHOTS) await page.screenshot({ path: `${process.env.TECTORA_SHOTS}r16-UX-1504-notice-after-12s-after.png` });
    // The edit's save replaces the file: the notice goes, "replaced" comes once.
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 31);
    await expect(replaced(app)).toHaveCount(1);
    await expect(notice(app)).toHaveCount(0);
    await app.saved();
  });

  test("a banner Revert over a file another program made unreadable says replaced (ERR-1502)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "R16 vrátit");
    await mcp.call("update_job", { id, customer: "AI zákazník", summary: "Zákazník od AI" });
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R16 vrátit");
    const review = page.locator(".review");
    await expect(review).toContainText("Zákazník od AI");
    writeTooDeep(path.join(LIBRARY_DIR, `${id}.tectora`));
    await expect(notice(app)).toHaveCount(1);
    await review.getByRole("button", { name: cs("ai.revert") }).first().click();
    const p = await waitForProject(id, (p) => p.job.customer === "");
    expect(["crm" in p, p.notes]).toEqual([false, ""]);
    await expect(replaced(app)).toHaveCount(1);
    await expect(notice(app)).toHaveCount(0);
    await app.saved();
    await expect(page.locator(".statusbar .save")).not.toContainText(cs("editor.outsideState"));
  });

  test("an unreadable file the other program repaired is not called replaced by the next save (TEST-1504)", async ({ app, page }) => {
    const id = await createExample("hip", "T16 opraveno");
    await openEditor(app, "T16 opraveno");
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const good = fs.readFileSync(file, "utf8");
    writeTooDeep(file);
    await expect(notice(app)).toHaveCount(1);
    // The other program writes a readable file again (its own note), which the editor reloads.
    const fixed = JSON.parse(good);
    fixed.notes = "opraveno venku";
    fs.writeFileSync(file, JSON.stringify(fixed));
    await expect(notice(app)).toHaveCount(0, { timeout: 10_000 });
    await expect(page.locator(".statusbar .save")).toHaveText(cs("editor.saved"));
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 31);
    await app.saved();
    await noToast(replaced(app), 2000, "nothing of the other program's file was replaced");
    expect(JSON.parse(fs.readFileSync(file, "utf8")).notes).toBe("opraveno venku");
  });

  test("refusals of a file a newer Tectora wrote name the job, also over the library after Back (UI-1503)", async ({ app, page }) => {
    const id = await createExample("hip", "R16 novější");
    await openEditor(app, "R16 novější");
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const newer = JSON.parse(fs.readFileSync(file, "utf8"));
    newer.schema_version = 99;
    const text = JSON.stringify(newer);
    fs.writeFileSync(file, text);
    // Round 17 (UI-1603 / HUNT-1604, owner decision 5): the reload of the
    // open job says what happened under its own title, not "could not open".
    const load = app.toasts().filter({ hasText: cs("err.ofJob", { name: "R16 novější", title: cs("editor.outsideTitle") }) }).filter({ hasText: cs("editor.outsideNewer") });
    await expect(load).toHaveCount(1);
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 33);
    const save = app.toasts().filter({ hasText: cs("err.ofJob", { name: "R16 novější", title: cs("err.action.save") }) });
    await expect(save).toHaveCount(1);
    await app.answerConfirm(true);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(page.locator("header .brand")).toBeVisible();
    // The lasting notice speaks about the open job: it goes with the editor;
    // the save refusal names the job and stays over the library.
    await expect(load).toHaveCount(0);
    await expect(save).toHaveCount(1);
    expect(fs.readFileSync(file, "utf8")).toBe(text);
    // The refusals are logged as errors by notifyError.
    app.errors.length = 0;
  });

  test("a window below a building is said while another building has no roof, and counted off the roof (HUNT-1502, UX-1503)", async ({ app, page }) => {
    const id = await createExample("gable", "R16 dvě budovy");
    const p = await loadProject(id);
    const b = structuredClone(p.parts[0]);
    b.name = "B";
    const pts = [[40, 0], [46, 0], [46, 6], [40, 6]];
    b.edges.forEach((e: { start: { x: number; y: number }; pitch_deg: number }, i: number) => {
      e.start = { x: pts[i][0], y: pts[i][1] };
      e.pitch_deg = 90;
    });
    p.parts.push(b);
    await bridge("save_project", { id, project: p });
    await app.open({ locale: "en" });
    await app.openJob("R16 dvě budovy");
    await page.getByRole("toolbar").first().getByRole("button", { name: "Roof window", exact: true }).click();
    await clickPlan(page, 6, -5);
    await waitForProject(id, (p) => p.windows?.length === 1);
    await expect(app.toasts().filter({ hasText: "Roof window 1" })).toHaveCount(1);
    await expect(page.getByTestId("results-warned")).toHaveCount(1);
    await page.locator("aside .tabs").getByRole("tab", { name: /^Results/ }).click();
    const objects = page.getByTestId("res-objects");
    await expect(objects).toContainText("Roof windows: 0");
    await expect(page.getByTestId("res-off-windows")).toHaveText("off the roof: 1");
  });

  test("two objects placed before one calculation answers are each checked, and Undo takes the warning back (UI-1502)", async ({ app, page }) => {
    const id = await createExample("hip", "R16 rychle");
    await openEditor(app, "R16 rychle");
    await app.saved();
    // Calculations answer late, as on a big job.
    await page.route("**/calc", async (route) => {
      const res = await route.fetch();
      await new Promise((r) => setTimeout(r, 1500));
      await route.fulfill({ response: res }).catch(() => {});
    });
    await tool(page, "tool.window").click();
    await clickPlan(page, 6, -5);
    await tool(page, "tool.chimney").click();
    await clickPlan(page, 3, 1.5);
    await waitForProject(id, (p) => p.windows?.length === 1 && p.chimneys?.length === 1);
    const warned = app.toasts().filter({ hasText: cs("res.warn.window_off_roof", { n: 1 }) });
    await expect(warned).toHaveCount(1, { timeout: 10_000 });
    await page.unroute("**/calc");
    // Undo both: the window the toast is about is gone, and so is the toast.
    for (let k = 0; k < 2; k++) await page.locator("body").press("ControlOrMeta+z");
    await waitForProject(id, (p) => !p.windows?.length && !p.chimneys?.length);
    await expect(warned).toHaveCount(0);
    await expect(page.getByTestId("results-warned")).toHaveCount(0);
  });

  test("a roof opening placed beside the building is said at once (TEST-1506)", async ({ app, page }) => {
    const id = await createExample("hip", "R16 otvor");
    await openEditor(app, "R16 otvor");
    await tool(page, "tool.hole").click();
    await clickPlan(page, 6, -4);
    await waitForProject(id, (p) => p.roof_openings?.length === 1);
    await expect(app.toasts().filter({ hasText: cs("res.warn.opening_off_roof", { n: 1 }) })).toHaveCount(1);
    await expect(page.getByTestId("results-warned")).toHaveCount(1);
  });

  test("the library card shows an AI's summary worded like Tectora's in quotation marks (TEST-1506)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "R16 karta");
    await mcp.call("update_job", { id, notes: "AI", summary: "Reverted: X" });
    await app.open({ prefs: { followAi: false } });
    await page.getByRole("row").filter({ hasText: "R16 karta" }).first().click();
    await expect(page.locator("aside.detail")).toContainText("„Reverted: X“");
  });
});
