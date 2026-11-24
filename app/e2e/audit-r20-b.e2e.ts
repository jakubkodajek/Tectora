// Guards for the round-20 audit (2026-11-24), second half.
//
// ERR-1902 the Export dialog shows the newest preview, never a slow answer of
//          the job before an AI save.
// ERR-1904 the library list shows the newest listing, never an older one.
// ERR-1905 a read of the review on its way when Accept is pressed never
//          brings the banner back.
// ERR-1906 two AI saves in a row: the editor ends on the newer job even when
//          the read of the first one is slow.
// ERR-1903 a failed review read keeps the banner; the next read updates it.
// HUNT-1903 the editor opens the job without waiting for the review's figures.
// TEST-1905 an AI save that lands between the editor's own post-save reads is
//          still undone through the core (the t19c window, made deterministic).
// UX-1904 / ERR-1901 a job of hand-drawn faces only lists its faces in
//          Results, shows no empty line list, and warns about a crossing face.
// UI-1901  at 600x420 the bars' More (and the dormer's Delete) never lie over
//          a field: a real click on the field lands in it, in all four languages.
import type { Page } from "@playwright/test";
import { bar, openEditor, selectWall, setField } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject, type Locale } from "./fixtures";
import { cs, LOCALES, tr } from "./i18n";
import { hold, timeline } from "./review";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r20";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r20-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

const review = (page: Page) => page.locator(".review");
const statusArea = (page: Page) => page.getByTestId("status-area");
const dockTab = (page: Page, key: string) => page.locator("aside .tabs").getByRole("tab", { name: cs(key) });
const undo = (page: Page) => page.locator("body").press("ControlOrMeta+z");
/** One frame, so an answer that just came in has been applied. */
const frame = (page: Page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 50))));
/** Waits until `n` answers to `cmd` arrived. */
const answers = async (t: { answered: number[] }, n: number) => expect.poll(() => t.answered.length, { timeout: 15_000 }).toBeGreaterThanOrEqual(n);

test.describe("round 20, design 2: an older answer never replaces a newer one", () => {
  test("the Export dialog keeps the newest preview when the first one answers last (ERR-1902)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R20 export");
    await openEditor(app, "R20 export");
    await app.saved();
    let n = 0;
    const held = await hold(page, "export_preview", () => n++ === 0);
    const sent = timeline(page, "export_preview");
    await app.toolbar(cs("tool.export")).click();
    const dlg = page.getByRole("dialog", { name: cs("export.title") });
    await expect(dlg).toBeVisible();
    await expect.poll(() => held.held()).toBe(1);
    await mcp.call("update_edges", { id, changes: [{ all: true, pitch_deg: 25 }], summary: "Sklon 25°" });
    await expect(dlg).toContainText("25,00°", { timeout: 10_000 });
    held.release();
    await answers(sent, 2);
    await frame(page);
    await expect(dlg).toContainText("25,00°");
    await expect(dlg).not.toContainText("40,00°");
    expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(25);
  });

  test("the library list keeps the newest listing when an older one answers last (ERR-1904)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R20 seznam");
    await app.open({ prefs: { followAi: false } });
    const row = page.getByRole("row").filter({ hasText: "R20 seznam" });
    await expect(row).toHaveCount(1);
    let armed = true;
    const held = await hold(page, "list_projects", () => {
      if (!armed) return false;
      armed = false;
      return true;
    });
    const sent = timeline(page, "list_projects");
    await mcp.call("update_job", { id, customer: "Zákazník X1", summary: "Zákazník 1" });
    await expect.poll(() => held.held(), { timeout: 10_000 }).toBe(1);
    await mcp.call("update_job", { id, customer: "Zákazník X2", summary: "Zákazník 2" });
    await expect(row).toContainText("Zákazník X2", { timeout: 10_000 });
    held.release();
    await answers(sent, 2);
    await frame(page);
    await expect(row).toContainText("Zákazník X2");
    await expect(row).not.toContainText("Zákazník X1");
  });

  test("two AI saves in a row, the read of the first one slow: the editor ends on the newer job (ERR-1906)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R20 načtení");
    await openEditor(app, "R20 načtení");
    await app.saved();
    await expect(statusArea(page)).toContainText("152,73");
    let armed = true;
    const held = await hold(page, "load_project", () => {
      if (!armed) return false;
      armed = false;
      return true;
    });
    const loads = timeline(page, "load_project");
    await mcp.call("update_job", { id, customer: "AI jedna", summary: "Zákazník od AI" });
    await expect.poll(() => held.held(), { timeout: 10_000 }).toBe(1);
    const sentBefore = loads.sent.length;
    await mcp.call("update_edges", { id, changes: [{ all: true, pitch_deg: 25 }], summary: "Sklon 25°" });
    // Single flight (audit 2026-11-24 TEST-2004, the test reviewer's
    // r21-load): two event polls after the second save are answered and no
    // second load_project went out while the first is held.
    for (let i = 0; i < 2; i++) await page.waitForResponse((r) => r.url().includes("__events"), { timeout: 10_000 });
    expect(loads.sent.length, "no second load_project while the first is out").toBe(sentBefore);
    held.release();
    // The coalesced read goes out once the first is in.
    await expect.poll(() => loads.sent.length, { timeout: 10_000 }).toBeGreaterThan(sentBefore);
    // The newer job, and it stays: every read sent has been answered and
    // applied, and no older one landed after it (no fixed wait).
    await expect(statusArea(page)).toContainText("129,10", { timeout: 15_000 });
    await expect.poll(() => loads.answered.length >= loads.sent.length, { timeout: 15_000 }).toBe(true);
    await frame(page);
    await expect(statusArea(page)).toContainText("129,10");
    await app.saved();
    const p = await loadProject(id);
    expect([p.parts[0].edges[0].pitch_deg, p.job.customer]).toEqual([25, "AI jedna"]);
  });

  test("a review read on its way when Accept is pressed does not bring the banner back (ERR-1905, ERR-1903)", async ({ app, page, mcp }) => {
    const created = await mcp.call("create_from_example", { example_id: "gable", name: "R20 přijetí", summary: "Založeno podle výkresu" });
    const id = created.id as string;
    await mcp.call("update_job", { id, notes: "A1", summary: "Poznámka od AI" });
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R20 přijetí");
    await expect(review(page)).toContainText("Poznámka od AI");
    // The next review read fails, the retry after it is held until after Accept.
    let calls = 0;
    let open!: () => void;
    const gate = new Promise<void>((r) => (open = r));
    let retried = 0;
    await page.route("**/review_status", async (route) => {
      calls++;
      if (calls === 1) {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ err: { code: "io", id, detail: "read failed" } }) });
        return;
      }
      if (calls === 2) {
        const res = await route.fetch();
        retried++;
        await gate;
        await route.fulfill({ response: res }).catch(() => {});
        return;
      }
      await route.continue();
    });
    await mcp.call("update_job", { id, customer: "A2", summary: "Zákazník od AI" });
    await expect.poll(() => calls, { timeout: 10_000 }).toBeGreaterThanOrEqual(1);
    // ERR-1903: the failed read keeps the banner and its buttons.
    await frame(page);
    await expect(review(page)).toContainText("Poznámka od AI");
    await expect(review(page).locator("button.accept")).toBeVisible();
    // The retry is out (held) when the user accepts.
    await expect.poll(() => retried, { timeout: 10_000 }).toBe(1);
    await review(page).locator("button.accept").click();
    await expect(review(page)).toHaveCount(0);
    expect((await mcp.call("get_review", { id })).pending).toBeNull();
    open();
    await page.waitForTimeout(800);
    await expect(review(page)).toHaveCount(0);
    const p = await loadProject(id);
    expect([p.notes, p.job.customer]).toEqual(["A1", "A2"]);
    app.errors.length = 0;
  });

  test("a failed review read keeps the banner and the next read brings it up to date (ERR-1903)", async ({ app, page, mcp }) => {
    const created = await mcp.call("create_from_example", { example_id: "gable", name: "R20 chyba čtení", summary: "Založeno podle výkresu" });
    const id = created.id as string;
    await mcp.call("update_job", { id, notes: "A1", summary: "Poznámka od AI" });
    await app.open({ prefs: { followAi: false } });
    await app.openJob("R20 chyba čtení");
    await expect(review(page)).toContainText("Poznámka od AI");
    let failed = 0;
    await page.route("**/review_status", async (route) => {
      if (failed === 0) {
        failed++;
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ err: { code: "io", id, detail: "read failed" } }) });
        return;
      }
      await route.continue();
    });
    await mcp.call("update_job", { id, customer: "A2", summary: "Zákazník od AI" });
    await expect.poll(() => failed, { timeout: 10_000 }).toBe(1);
    await frame(page);
    await expect(review(page)).toContainText("Poznámka od AI");
    // The one retry reads the review again: both steps are listed.
    await expect(review(page)).toContainText("Zákazník od AI", { timeout: 10_000 });
    app.errors.length = 0;
  });
});

test.describe("round 20, HUNT-1903: the job opens without waiting for the review", () => {
  test("a slow review read does not keep the job from opening; the banner comes when it is in (HUNT-1903)", async ({ app, page, mcp }) => {
    const created = await mcp.call("create_from_example", { example_id: "gable", name: "R20 pomalá kontrola", summary: "Založeno podle výkresu" });
    const id = created.id as string;
    await mcp.call("update_job", { id, notes: "A1", summary: "Poznámka od AI" });
    await app.open({ prefs: { followAi: false } });
    const held = await hold(page, "review_status");
    await app.openJob("R20 pomalá kontrola");
    await expect.poll(() => held.held(), { timeout: 10_000 }).toBeGreaterThanOrEqual(1);
    // The job is shown: plan, figures and the Job tab's notes.
    await expect(page.locator(".view .wrap > svg g.edge line.hit").first()).toBeAttached();
    await expect(statusArea(page)).toContainText("152,73");
    await dockTab(page, "dock.job").click();
    await expect(page.getByRole("tabpanel").getByLabel(cs("job.notes"), { exact: true })).toHaveValue("A1");
    await expect(review(page)).toHaveCount(0);
    held.release();
    await expect(review(page)).toContainText("Poznámka od AI", { timeout: 10_000 });
    expect((await loadProject(id)).notes).toBe("A1");
  });
});

test.describe("round 20, TEST-1905: an AI save between the editor's own post-save reads", () => {
  test("Ctrl+Z over that AI save goes through the core and is logged (TEST-1905, the t19c window)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "R20 závod");
    await openEditor(app, "R20 závod");
    await app.saved();
    let holding = false;
    let held = 0;
    let open!: () => void;
    const gate = new Promise<void>((r) => (open = r));
    await page.route("**/project_activity", async (route) => {
      if (holding) {
        held++;
        holding = false;
        // Read only after the AI's entry is logged.
        await gate;
      }
      await route.continue();
    });
    let calls = 0;
    await page.route("**/revert_changes", async (route) => {
      calls++;
      await route.continue();
    });
    await selectWall(page, 0);
    holding = true;
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 33);
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 33);
    // The editor's own reload is now between its read of the job and of the log.
    await expect.poll(() => held, { timeout: 10_000 }).toBe(1);
    await mcp.call("update_job", { id, notes: "Od AI", summary: "Poznámka od AI" });
    open();
    await expect(review(page)).toContainText("Poznámka od AI");
    await expect(async () => {
      await dockTab(page, "dock.job").click();
      await expect(page.getByRole("tabpanel").getByLabel(cs("job.notes"), { exact: true })).toHaveValue("Od AI", { timeout: 500 });
    }).toPass({ timeout: 8000 });
    await undo(page);
    await expect.poll(() => calls, { timeout: 10_000 }).toBe(1);
    const p = await waitForProject(id, (q) => !q.notes);
    expect(p.parts[0].edges[0].pitch_deg).toBe(33);
    const h = await mcp.call("get_history", { id });
    expect([h[0].kind, h[0].actor]).toEqual(["revert", "user"]);
    await expect(review(page)).toHaveCount(0);
  });
});

test.describe("round 20, UX-1904 and ERR-1901: a job of hand-drawn faces only", () => {
  test("Results lists the faces, shows no empty line list and warns about a crossing face (UX-1904, ERR-1901)", async ({ app, page }) => {
    const id = await createExample("gable", "R20 jen plochy");
    const p = await loadProject(id);
    p.parts = [];
    p.dormers = [];
    p.windows = [];
    p.chimneys = [];
    p.roof_openings = [];
    p.manual_faces = [
      { points: [{ x: 0, y: 0 }, { x: 6, y: 0 }, { x: 6, y: 4 }, { x: 0, y: 4 }], pitch_deg: 30, label: "" },
      // Stored by an older Tectora: corners out of order (a bow tie).
      { points: [{ x: 10, y: 0 }, { x: 14, y: 4 }, { x: 14, y: 0 }, { x: 10, y: 4 }], pitch_deg: 30, label: "" },
    ];
    await bridge("save_project", { id, project: p });
    await app.open();
    await app.openJob("R20 jen plochy");
    await dockTab(page, "dock.results").click();
    const res = page.locator(".results");
    await expect(res).toContainText(cs("manual.faceN", { n: 1 }));
    await expect(res).toContainText(cs("manual.faceN", { n: 2 }));
    await expect(page.getByTestId("res-legend")).toHaveCount(0);
    await expect(page.getByTestId("res-lines")).toHaveCount(0);
    await expect(res.locator(".warn")).toContainText(cs("res.warn.manual_face_crosses", { n: 2 }));
  });
});

/** Whether a real click at the centre of `field` lands in it (nothing lies over it). */
async function clickLandsIn(page: Page, field: import("@playwright/test").Locator): Promise<string> {
  await field.scrollIntoViewIfNeeded();
  const box = (await field.boundingBox())!;
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  const hit = await page.evaluate(([x, y]) => {
    const e = document.elementFromPoint(x, y) as HTMLElement | null;
    return e ? `${e.tagName}.${e.className}` : "none";
  }, [x, y]);
  await page.mouse.click(x, y);
  const focused = await field.evaluate((f) => f === document.activeElement);
  return focused ? "ok" : `covered by ${hit}`;
}

test.describe("round 20, UI-1901: More never lies over a field at 600x420", () => {
  for (const locale of LOCALES) {
    test(`the wall bar's fields and the gable dormer's pitch take a real click at 600x420 (UI-1901, ${locale})`, async ({ app, page }) => {
      await page.setViewportSize({ width: 600, height: 420 });
      await createExample("hip", "R20 Více");
      await app.open({ locale: locale as Locale, prefs: { wallMore: false, dormerMore: false } });
      await app.openJob("R20 Více");
      await page.locator(".view .wrap > svg g.edge line.hit").nth(1).dispatchEvent("pointerdown", { button: 0 });
      const wall = page.getByRole("toolbar", { name: tr(locale as Locale, "def.title") });
      await expect(wall.getByTestId("def-more")).toBeVisible();
      for (const key of ["edge.pitch", "edge.overhang"]) {
        const field = wall.getByLabel(tr(locale as Locale, key));
        expect(await clickLandsIn(page, field), `${locale} ${key}`).toBe("ok");
      }
      // The scrolled strip and More side by side: More is not inside the strip.
      const inside = await wall.getByTestId("def-more").evaluate((b) => !!b.closest(".row"));
      expect(inside).toBe(false);
    });
  }

  for (const locale of LOCALES) {
    test(`the gable dormer's pitch takes a real click at 600x420 (UI-1901, ${locale})`, async ({ app, page, mcp }) => {
      const id = await createExample("gable", "R20 vikýř");
      await mcp.call("add_dormer", { id, summary: "Vikýř", kind: "gable", start: [2, 1] });
      await page.setViewportSize({ width: 600, height: 420 });
      await app.open({ locale: locale as Locale, prefs: { wallMore: false, dormerMore: false, followAi: false } });
      await app.openJob("R20 vikýř");
      await page.locator("g.dormer line.hit").first().dispatchEvent("pointerdown", { button: 0 });
      const db = page.getByRole("toolbar", { name: tr(locale as Locale, "dormer.title") });
      await expect(db.getByTestId("dormer-more")).toBeVisible();
      const field = db.getByLabel(tr(locale as Locale, "edge.pitch"));
      expect(await clickLandsIn(page, field), `${locale} dormer pitch`).toBe("ok");
      const inside = await db.getByTestId("dormer-more").evaluate((b) => !!b.closest(".row"));
      expect(inside).toBe(false);
    });
  }

  for (const locale of LOCALES) {
    test(`the closed wall bar's strip fits one row at 960x540 without scrolling (UI-1901, ${locale})`, async ({ app, page }) => {
      await page.setViewportSize({ width: 960, height: 540 });
      await createExample("hip", "R20 Valba");
      await app.open({ locale: locale as Locale, prefs: { wallMore: false } });
      await app.openJob("R20 Valba");
      await page.locator(".view .wrap > svg g.edge line.hit").first().dispatchEvent("pointerdown", { button: 0 });
      const strip = page.getByTestId("def-more").locator("xpath=..").locator(":scope > .row");
      await expect(strip).toHaveCount(1);
      const { client, scroll } = await strip.evaluate((r) => ({ client: r.clientWidth, scroll: r.scrollWidth }));
      expect(scroll, `strip overflows by ${scroll - client}px`).toBeLessThanOrEqual(client);
    });
  }
});
