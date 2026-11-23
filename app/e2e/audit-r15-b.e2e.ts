// Guards for the round-15 audit (2026-11-23), part B (notices, limits, wording).
//
// ERR-1401  an open job another program made unreadable: the notice says what
//           the next save does, "replaced" after it; leaving without an edit
//           writes nothing
// ERR-1403  two jobs of one name deleted: two toasts, each Restore brings its
//           own job back
// UX-1401   a window or chimney placed on no roof face: a warning at once and
//           the Results tab's mark, from the core's warning
import fs from "node:fs";
import path from "node:path";
import { bar, clickPlan, openEditor, selectWall, setField, tool } from "./editor";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, listProjects, noToast, test, waitForProject } from "./fixtures";
import { cs, tr } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r15";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r15-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

/** `{"a": {"a": … {}}}` with `n` objects inside each other. */
function nestedObjects(n: number): unknown {
  let v: unknown = {};
  for (let k = 1; k < n; k++) v = { a: v };
  return v;
}

/** The job file with 40 levels of unknown keys another program wrote. */
function writeTooDeep(file: string): string {
  const outside = JSON.parse(fs.readFileSync(file, "utf8"));
  outside.crm = nestedObjects(40);
  const text = JSON.stringify(outside);
  fs.writeFileSync(file, text);
  return text;
}

test.describe("round 15, part B", () => {
  test("an open job made too deep to read from outside says the next save replaces it, and says so after it (ERR-1401)", async ({ app, page }) => {
    const id = await createExample("hip", "R15 cizí");
    await openEditor(app, "R15 cizí");
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    writeTooDeep(file);
    const notice = app.toasts().filter({ hasText: cs("editor.outsideUnreadable") });
    await expect(notice).toHaveCount(1);
    await expect(notice).toContainText("R15 cizí");
    // Not the open-time advice: remove the data in that program, or a backup.
    await expect(app.toasts().filter({ hasText: cs("err.unknown_keys") })).toHaveCount(0);
    await expect(app.toasts().filter({ hasText: cs("err.unreadable") })).toHaveCount(0);
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 31);
    await app.saved();
    await expect(app.toasts().filter({ hasText: cs("editor.outsideReplaced") })).toHaveCount(1);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    expect([p.parts[0].edges[0].pitch_deg, "crm" in p]).toEqual([31, false]);
  });

  test("leaving an open job made unreadable from outside without an edit keeps that program's file (ERR-1401)", async ({ app, page }) => {
    const id = await createExample("hip", "R15 cizí zpět");
    await openEditor(app, "R15 cizí zpět");
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const text = writeTooDeep(file);
    await expect(app.toasts().filter({ hasText: cs("editor.outsideUnreadable") })).toHaveCount(1);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(page.locator("header .brand")).toBeVisible();
    await noToast(app.toasts().filter({ hasText: cs("editor.outsideReplaced") }));
    expect(fs.readFileSync(file, "utf8")).toBe(text);
  });

  test("two jobs of one name deleted give two toasts, and each Restore brings its own job back (ERR-1403)", async ({ app, page }) => {
    const a = await createExample("hip", "R15 dvojče");
    const b = await createExample("gable", "R15 dvojče");
    expect(a).not.toBe(b);
    await app.open();
    const detail = page.locator("aside.detail");
    for (let k = 0; k < 2; k++) {
      await app.answerConfirm(true);
      await page.getByRole("row").filter({ hasText: "R15 dvojče" }).first().click();
      await detail.getByRole("button", { name: cs("card.delete") }).click();
      await expect.poll(async () => (await listProjects()).length).toBe(1 - k);
    }
    const deleted = app.toasts().filter({ hasText: cs("home.deleted", { name: "R15 dvojče" }) });
    await expect(deleted).toHaveCount(2);
    for (let k = 0; k < 2; k++) await deleted.first().getByRole("button", { name: cs("home.restore") }).click();
    await expect.poll(async () => (await listProjects()).map((p) => p.id).sort()).toEqual([a, b].sort());
    await expect.poll(async () => (await bridge<unknown[]>("list_deleted")).length).toBe(0);
  });

  test("a roof window placed below the building is said at once and marks the Results tab, in de at 600 x 420 (UX-1401)", async ({ app, page }) => {
    const id = await createExample("hip", "R15 mimo");
    await app.open({ locale: "de" });
    await app.openJob("R15 mimo");
    const de = (key: string, vars?: Record<string, string | number>) => tr("de", key, vars);
    const winTool = page.getByRole("toolbar").first().getByRole("button", { name: de("tool.window"), exact: true });
    const mark = page.getByRole("tab", { name: de("dock.resultsWarned") });
    // On a face: neither the warning nor the mark.
    await winTool.click();
    await clickPlan(page, 3, 1.5);
    await waitForProject(id, (p) => p.windows?.length === 1);
    await noToast(app.toasts().filter({ hasText: de("res.warn.title") }));
    await expect(page.getByTestId("results-warned")).toHaveCount(0);
    // Below the building: kept, as MCP keeps it, and said at once.
    await winTool.click();
    await clickPlan(page, 6, -5);
    await waitForProject(id, (p) => p.windows?.length === 2);
    const warned = app.toasts().filter({ hasText: de("res.warn.window_off_roof", { n: 2 }) });
    await expect(warned).toHaveCount(1);
    await expect(warned).toContainText(de("res.warn.title"));
    await expect(mark).toHaveCount(1);
    await page.setViewportSize({ width: 600, height: 420 });
    await expect(page.getByTestId("results-warned")).toBeVisible();
    await expect(mark).toBeVisible();
  });

  test("a chimney placed beside the building is said at once (UX-1401)", async ({ app, page }) => {
    const id = await createExample("hip", "R15 komín");
    await openEditor(app, "R15 komín");
    await tool(page, "tool.chimney").click();
    await clickPlan(page, 6, -4);
    await waitForProject(id, (p) => p.chimneys?.length === 1);
    await expect(app.toasts().filter({ hasText: cs("res.warn.chimney_off_roof", { n: 1 }) })).toHaveCount(1);
    await expect(page.getByTestId("results-warned")).toHaveCount(1);
  });
});
