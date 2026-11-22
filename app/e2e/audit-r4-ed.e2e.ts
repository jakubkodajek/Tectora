// Guards for the round-4 audit (2026-11-22) findings in the editor. They all
// test one rule (app/src/lib/jobedits.ts): an edit that waits for the core
// belongs to the item it was made for, edits run one after another, an
// outside save refuses only the edit whose own item it changed, a one-shot
// action acts once per press, and a field keeps the last value set.
//
// ERR-301 an edit or a status change after an ordinary edit plus an AI save
//   of the notes; ERR-302 a dormer width with an AI delete during the check;
// ERR-303 re-offer with a kept result on a roof the core refuses; ERR-304
// the kept-result note names what differs; ERR-307 Make it a building with
// an AI building added meanwhile; UI-301 quick steps and two fields in the
// wall and chimney bars; UI-302, UI-308 double clicks on OK; UI-304 a double
// click on a button that opens a dialog; UI-307 a step up then down.
import type { Locator, Page, Route } from "@playwright/test";
import { bar, clickPlan, openEditor, planXY, selectWall, setField, tool } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject, type App } from "./fixtures";
import { cs } from "./i18n";

const square = (x: number, y: number, s: number) => [
  { x, y },
  { x: x + s, y },
  { x: x + s, y: y + s },
  { x, y: y + s },
];
const extent = (o: any, k: "x" | "y") => +(Math.max(...o.corners.map((c: any) => c[k])) - Math.min(...o.corners.map((c: any) => c[k]))).toFixed(3);
const dormerWidth = (d: any) => +Math.hypot(d.end.x - d.edges[0].start.x, d.end.y - d.edges[0].start.y).toFixed(3);
const dormerSpans = (p: any) => p.dormers.map((d: any) => `${d.edges[0].start.x}:${dormerWidth(d)}`);
const wallLength = (p: any, part: number, edge: number) => {
  const es = p.parts[part].edges;
  const a = es[edge].start;
  const b = es[(edge + 1) % es.length].start;
  return +Math.hypot(b.x - a.x, b.y - a.y).toFixed(3);
};
const jobTab = (page: Page) => page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") });

async function withItems(id: string, edit: (p: any) => void) {
  const p = await loadProject(id);
  edit(p);
  await bridge("save_project", { id, project: p });
}

/** Types like a person: click, select all, type (no fill()). */
async function typeInto(page: Page, field: Locator, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

/** Holds back the bridge's answers to `cmd` by `ms` while `on()` is true. */
async function slow(page: Page, cmd: string, ms: number, on: () => boolean = () => true) {
  await page.route(new RegExp(`/${cmd}$`), async (r: Route) => {
    if (on()) await new Promise((res) => setTimeout(res, ms));
    await r.continue();
  });
}

/** Every toast text seen from now on (toasts close themselves). */
async function watchToasts(page: Page): Promise<() => Promise<string[]>> {
  await page.evaluate(() => {
    const w = window as any;
    w.__toasts = [];
    const seen = new WeakSet<Element>();
    const scan = () => document.querySelectorAll(".stack .toast").forEach((t) => !seen.has(t) && (seen.add(t), w.__toasts.push((t.textContent ?? "").replace(/\s+/g, " ").trim())));
    scan();
    new MutationObserver(scan).observe(document.body, { childList: true, subtree: true });
  });
  return () => page.evaluate(() => (window as any).__toasts as string[]);
}

/** Accepts the AI changes made so far, as the banner's Accept does. */
async function acceptAll(id: string) {
  const log = await bridge<{ ts_ms: number }[]>("project_activity", { id });
  if (log.length) await bridge("accept_changes", { id, tsMs: Math.max(...log.map((a) => a.ts_ms)) });
}

async function threeDormers(mcp: any, id: string) {
  for (const x of [1.5, 5, 8.5]) await mcp.call("add_dormer", { id, kind: "gable", start: [x, 1.6], width: 1.5, summary: "Dormer." });
  await acceptAll(id);
}

/** Opens `name` in the editor after AI saves: follow mode may have opened it already. */
async function openAfterAi(app: App, name: string) {
  await app.open();
  const editor = app.page.locator(".editor .statusbar");
  if (!(await editor.isVisible().catch(() => false))) await app.page.getByRole("row").filter({ hasText: name }).first().dblclick({ timeout: 3000 }).catch(() => {});
  await expect(editor).toBeVisible();
  await expect(app.page.locator(".titlebar, .editor").first()).toBeVisible();
  await expect(app.page.locator(".view .wrap > svg").first()).toBeVisible();
}

async function selectDormer(page: Page, n: number) {
  const db = bar(page, "dormer.title");
  const bb = (await page.locator("g.dormer path.dormer-area").first().boundingBox())!;
  await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
  await expect(db.locator(".which strong")).toBeVisible();
  for (let i = 0; i < 4 && (await db.locator(".which strong").textContent()) !== cs("dormer.n", { n: 1 }); i++) await db.getByRole("button", { name: cs("dormer.prev") }).click();
  for (let i = 1; i < n; i++) await db.getByRole("button", { name: cs("dormer.next") }).click();
  await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n }));
  return db;
}

async function selectChimney(page: Page, n = 1) {
  const bb = (await page.locator("path.opening.chimney").nth(n - 1).boundingBox())!;
  await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
  const ob = bar(page, "opening.chimney");
  await expect(ob.locator(".which strong")).toHaveText(cs("opening.chimneyN", { n }));
  return ob;
}

// With TECTORA_SHOTS=<folder>/ and TECTORA_SHOTS_TAG=before|after every test
// leaves a screenshot `r4-<finding>-<n>-<tag>.png` there (the audit's
// before / after pictures); without it nothing is written.
let shotNo = 0;
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/\(([A-Z]+-\d+)/)?.[1] ?? "misc";
  await page.screenshot({ path: `${dir}r4-${id}-${++shotNo}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png` }).catch(() => {});
});

test.describe("round-4 audit: an edit belongs to its item", () => {
  test("a dormer width typed while the AI deletes an earlier dormer lands on its own dormer, and the bar follows it (ERR-302)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Vikýře a AI");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Vikýře a AI");
    const db = await selectDormer(page, 2);
    const toasts = await watchToasts(page);
    let delay = true;
    await slow(page, "check_dormer", 2500, () => delay);
    await db.getByLabel(cs("dormer.width")).fill("2");
    await db.getByLabel(cs("dormer.width")).press("Enter");
    await page.waitForTimeout(500);
    await mcp.call("delete_items", { id, items: [{ kind: "dormer", index: 0 }], summary: "AI removes dormer 1." });
    await waitForProject(id, (p) => p.dormers.length === 2);
    delay = false;
    const p = await waitForProject(id, (p) => p.dormers.length === 2 && p.dormers.some((d: any) => dormerWidth(d) === 2), 12_000);
    expect(dormerSpans(p)).toEqual(["5:2", "8.5:1.5"]);
    // The bar is still on the dormer the user edits, now the first of two.
    await expect(db.locator(".which strong")).toHaveText(cs("dormer.n", { n: 1 }));
    await expect(db.getByLabel(cs("dormer.width"))).toHaveValue("2");
    expect((await toasts()).filter((t) => t.includes(cs("editor.actionStale")))).toEqual([]);
  });

  test("a dormer width typed for the very dormer the AI deletes meanwhile is refused with a note, not given to its neighbour (ERR-302)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Vikýř pryč");
    await threeDormers(mcp, id);
    await openAfterAi(app, "Vikýř pryč");
    const db = await selectDormer(page, 2);
    let delay = true;
    await slow(page, "check_dormer", 2500, () => delay);
    await db.getByLabel(cs("dormer.width")).fill("2,9");
    await db.getByLabel(cs("dormer.width")).press("Tab");
    await page.waitForTimeout(500);
    await mcp.call("delete_items", { id, items: [{ kind: "dormer", index: 1 }], summary: "AI removes dormer 2." });
    await waitForProject(id, (p) => p.dormers.length === 2);
    delay = false;
    await expect(app.toasts().filter({ hasText: cs("editor.actionStale") })).toBeVisible({ timeout: 12_000 });
    await page.waitForTimeout(1500);
    expect(dormerSpans(await loadProject(id))).toEqual(["1.5:1.5", "8.5:1.5"]);
    // The deleted dormer is no longer selected: the bar does not show its neighbour as "dormer 2".
    await expect(db).toHaveCount(0);
  });

  test("Duplicate after an ordinary edit still makes its copy when the AI saves only the notes meanwhile (ERR-301)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Kopie po úpravě");
    await openEditor(app, "Kopie po úpravě");
    // An ordinary edit first: the editor's copy of the job now differs in form from the file.
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    await def.getByRole("radio", { name: cs("edge.gable") }).click();
    await def.getByRole("radio", { name: cs("edge.eave") }).click();
    await app.saved();
    const toasts = await watchToasts(page);
    let delay = true;
    await slow(page, "transform_part", 2500, () => delay);
    const at = await planXY(page, 3, 3);
    await page.mouse.click(at.x, at.y, { button: "right" });
    await page.getByRole("menuitem", { name: cs("menu.duplicatePart") }).click();
    await page.waitForTimeout(500);
    await mcp.call("update_job", { id, notes: "AI note", summary: "AI adds a note." });
    await waitForProject(id, (p) => p.notes === "AI note");
    delay = false;
    const p = await waitForProject(id, (p) => p.parts.length === 2, 12_000);
    expect(p.notes).toBe("AI note");
    expect((await toasts()).filter((t) => t.includes(cs("editor.actionStale")))).toEqual([]);
  });

  test("a status change after an ordinary edit goes through when the AI saves only the notes meanwhile (ERR-301)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Stav po úpravě");
    await openEditor(app, "Stav po úpravě");
    await tool(page, "tool.flat").click();
    await tool(page, "tool.flat").click();
    await app.saved();
    await slow(page, "take_snapshot", 2500);
    await jobTab(page).click();
    await page.locator("aside .job select").selectOption("offer");
    await page.waitForTimeout(400);
    await mcp.call("update_job", { id, notes: "AI note", summary: "AI adds a note." });
    const p = await waitForProject(id, (p) => p.job.status === "offer" && !!p.result_snapshot, 12_000);
    expect(p.notes).toBe("AI note");
    await expect(app.toasts().filter({ hasText: cs("job.statusStale") })).toHaveCount(0);
  });

  test("Make it a building while the AI adds a building: the new one is selected and numbered after the AI's (ERR-307)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Plocha a AI");
    await withItems(id, (p) => (p.manual_faces = [{ points: [{ x: 9, y: 1 }, { x: 12.5, y: 1 }, { x: 12.5, y: 7 }, { x: 9, y: 7 }], pitch_deg: 30 }]));
    await openEditor(app, "Plocha a AI");
    const bb = (await page.locator(".manual-face").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    const mb = bar(page, "manual.face");
    let delay = true;
    await slow(page, "face_to_part", 2500, () => delay);
    await mb.getByRole("button", { name: cs("manual.toPart") }).click();
    await page.waitForTimeout(500);
    await mcp.call("add_part", { id, part: { name: "Garage", points: [[0, 12], [6, 12], [6, 17], [0, 17]] }, summary: "AI adds a garage." });
    await waitForProject(id, (p) => p.parts.length === 2);
    delay = false;
    const p = await waitForProject(id, (p) => p.parts.length === 3 && !p.manual_faces?.length, 12_000);
    expect(p.parts.map((x: any) => x.name)).toEqual([p.parts[0].name, "Garage", cs("editor.partN", { n: 3 })]);
    // The wall bar is on the building the user made, not on the AI's.
    await expect(bar(page, "def.title")).toContainText(cs("editor.partN", { n: 3 }));
    await expect(bar(page, "def.title")).not.toContainText("Garage");
  });
});

test.describe("round-4 audit: edits run one after another", () => {
  const withChimney = async (name: string) => {
    const id = await createExample("gable", name);
    await withItems(id, (p) => (p.chimneys = [{ corners: square(5, 1.5, 0.6), height: 8 }]));
    return id;
  };

  test("four quick ArrowUp steps of a chimney's width and X all land, with no note (UI-301)", async ({ app, page }) => {
    const id = await withChimney("Kroky komína");
    await openEditor(app, "Kroky komína");
    const ob = await selectChimney(page);
    const toasts = await watchToasts(page);
    await slow(page, "item_geometry", 300);
    const width = ob.getByLabel(cs("opening.width"));
    await width.click();
    for (let i = 0; i < 4; i++) await width.press("ArrowUp");
    await waitForProject(id, (p) => extent(p.chimneys[0], "x") === 0.8, 10_000);
    const x = ob.getByRole("textbox", { name: /^X/ });
    await x.click();
    for (let i = 0; i < 4; i++) await x.press("ArrowUp");
    await waitForProject(id, (p) => Math.min(...p.chimneys[0].corners.map((c: any) => c.x)) === 5.2, 10_000);
    await expect(width).toHaveValue("0,8");
    expect(await toasts()).toEqual([]);
  });

  test("four quick ArrowUp steps of the building height and of a wall length all land, with no note (UI-301)", async ({ app, page }) => {
    const id = await createExample("gable", "Kroky stěny");
    await openEditor(app, "Kroky stěny");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const toasts = await watchToasts(page);
    const h0 = (await loadProject(id)).parts[0].edges[0].eave_height;
    await slow(page, "set_part_height", 300);
    await slow(page, "item_geometry", 300);
    const height = def.getByTestId("building-height");
    await height.click();
    for (let i = 0; i < 4; i++) await height.press("ArrowUp");
    await waitForProject(id, (p) => p.parts[0].edges[0].eave_height === +(h0 + 0.2).toFixed(3), 10_000);
    const l0 = wallLength(await loadProject(id), 0, 0);
    const length = def.getByLabel(cs("edge.length"));
    await length.click();
    for (let i = 0; i < 4; i++) await length.press("ArrowUp");
    await waitForProject(id, (p) => wallLength(p, 0, 0) === +(l0 + 0.2).toFixed(3), 10_000);
    expect(await toasts()).toEqual([]);
  });

  test("a chimney width then its depth typed while the first is with the core both land (UI-301)", async ({ app, page }) => {
    const id = await withChimney("Dvě pole komína");
    await openEditor(app, "Dvě pole komína");
    const ob = await selectChimney(page);
    const toasts = await watchToasts(page);
    await slow(page, "item_geometry", 600);
    await typeInto(page, ob.getByLabel(cs("opening.width")), "0,8");
    await page.keyboard.press("Tab");
    await typeInto(page, ob.getByLabel(cs("opening.depth")), "0,9");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => extent(p.chimneys[0], "x") === 0.8 && extent(p.chimneys[0], "y") === 0.9, 10_000);
    expect(await toasts()).toEqual([]);
  });

  test("a wall length then a pitch typed while the length is with the core both land (UI-301)", async ({ app, page }) => {
    const id = await createExample("gable", "Délka a sklon");
    await openEditor(app, "Délka a sklon");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const toasts = await watchToasts(page);
    await slow(page, "item_geometry", 600);
    await typeInto(page, def.getByLabel(cs("edge.length")), "12,4");
    await page.keyboard.press("Tab");
    await typeInto(page, def.getByLabel(cs("edge.pitch")), "37");
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => wallLength(p, 0, 0) === 12.4 && p.parts[0].edges[0].pitch_deg === 37, 10_000);
    expect(await toasts()).toEqual([]);
  });

  test("ArrowUp then ArrowDown on the building height leaves the height as it was (UI-307)", async ({ app, page }) => {
    const id = await createExample("gable", "Nahoru a dolů");
    await openEditor(app, "Nahoru a dolů");
    const def = bar(page, "def.title");
    await selectWall(page, 0);
    const h0 = (await loadProject(id)).parts[0].edges[0].eave_height;
    await slow(page, "set_part_height", 400);
    const height = def.getByTestId("building-height");
    await height.click();
    await height.press("ArrowUp");
    await height.press("ArrowDown");
    await page.waitForTimeout(2500);
    await app.saved();
    expect((await loadProject(id)).parts[0].edges[0].eave_height).toBe(h0);
    await expect(height).toHaveValue(String(h0).replace(".", ","));
  });

  test("a dormer width typed and then typed back while the check is out keeps the value typed last (UI-307)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Zpět na šířku");
    await mcp.call("add_dormer", { id, kind: "gable", start: [4, 1.6], width: 2, summary: "Dormer." });
    await acceptAll(id);
    await openAfterAi(app, "Zpět na šířku");
    const db = await selectDormer(page, 1);
    await slow(page, "check_dormer", 500);
    const width = db.getByLabel(cs("dormer.width"));
    await typeInto(page, width, "2,6");
    await page.keyboard.press("Enter");
    await typeInto(page, width, "2");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(3000);
    await app.saved();
    expect(dormerWidth((await loadProject(id)).dormers[0])).toBe(2);
    await expect(width).toHaveValue("2");
  });
});

test.describe("round-4 audit: a one-shot action acts once per press", () => {
  test("a double click on OK of the building dialog (move with one copy) makes one copy (UI-302)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvojklik kopie");
    await openEditor(app, "Dvojklik kopie");
    await selectWall(page, 0);
    await tool(page, "tool.transform").click();
    const dlg = page.getByRole("dialog", { name: cs("xf.title") });
    await setField(dlg.getByLabel(cs("xf.dy")), 14);
    await setField(dlg.getByTestId("xf-copies"), 1);
    await dlg.getByRole("button", { name: cs("common.ok") }).dblclick();
    await waitForProject(id, (p) => p.parts.length >= 2);
    await page.waitForTimeout(1500);
    await app.saved();
    expect((await loadProject(id)).parts).toHaveLength(2);
  });

  test("a double click on OK of the calculation helper applies it once, with no note (UI-308)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvojklik výpočtu");
    await openEditor(app, "Dvojklik výpočtu");
    const e0 = (await loadProject(id)).parts[0].edges[0];
    await selectWall(page, 0);
    const toasts = await watchToasts(page);
    await bar(page, "def.title").getByRole("button", { name: cs("calc.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("calc.title") });
    await dlg.getByRole("tab", { name: cs("calc.buildup") }).click();
    await setField(dlg.getByRole("textbox", { name: cs("calc.thickness") }), "0,3");
    await dlg.getByRole("button", { name: cs("common.ok") }).dblclick();
    await waitForProject(id, (p) => p.parts[0].edges[0].overhang !== e0.overhang);
    await page.waitForTimeout(1500);
    await expect(dlg).toHaveCount(0);
    expect(await toasts()).toEqual([]);
  });

  test("a double click on Make it a building converts the face once, with no note (UI-308)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvojklik plochy");
    await withItems(
      id,
      (p) =>
        (p.manual_faces = [
          { points: [{ x: 9, y: 1 }, { x: 12.5, y: 1 }, { x: 12.5, y: 3.5 }, { x: 9, y: 3.5 }], pitch_deg: 30 },
          { points: [{ x: 9, y: 4.5 }, { x: 12.5, y: 4.5 }, { x: 12.5, y: 7 }, { x: 9, y: 7 }], pitch_deg: 30 },
        ]),
    );
    await openEditor(app, "Dvojklik plochy");
    const bb = (await page.locator(".manual-face").first().boundingBox())!;
    await page.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2);
    const toasts = await watchToasts(page);
    await slow(page, "face_to_part", 300);
    await bar(page, "manual.face").getByRole("button", { name: cs("manual.toPart") }).dblclick();
    await waitForProject(id, (p) => p.parts.length === 2);
    await page.waitForTimeout(1500);
    await app.saved();
    const p = await loadProject(id);
    expect([p.parts.length, p.manual_faces.length]).toEqual([2, 1]);
    expect(await toasts()).toEqual([]);
  });

  test("a double click on the empty job's Standard roof button leaves its dialog open (UI-304)", async ({ app, page }) => {
    const id = await createExample("gable", "Dvojklik okna");
    await withItems(id, (p) => ((p.parts = []), (p.dormers = []), (p.chimneys = [])));
    await openEditor(app, "Dvojklik okna");
    await page.keyboard.press("Escape");
    const def = bar(page, "def.title");
    await def.getByRole("button", { name: cs("tool.standard") }).dblclick();
    await page.waitForTimeout(600);
    await expect(page.getByRole("dialog")).toHaveCount(1);
    // A press on the backdrop later still closes it.
    await page.mouse.click(5, 5);
    await expect(page.getByRole("dialog")).toHaveCount(0);
    void clickPlan;
  });
});

test.describe("round-4 audit: re-offer with a kept result", () => {
  const offered = async (mcp: any, name: string) => {
    const id = await createExample("gable", name);
    await mcp.call("update_job", { id, status: "offer", summary: "Offer." });
    await mcp.call("update_job", { id, status: "draft", summary: "Back to draft." });
    await acceptAll(id);
    return id;
  };

  test("the status stays a draft and the refusal is shown when the core leaves a building out (ERR-303)", async ({ app, page, mcp }) => {
    const id = await offered(mcp, "Ponechaný výsledek");
    await openAfterAi(app, "Ponechaný výsledek");
    await jobTab(page).click();
    let delay = true;
    await slow(page, "calc", 5000, () => delay);
    // An outside save adds a 4 mm wall: the core leaves the building out.
    await withItems(id, (p) => {
      const e = p.parts[0].edges;
      e.splice(1, 0, { ...e[0], start: { x: e[1].start.x - 0.004, y: e[1].start.y } });
    });
    await expect(page.locator(".statusbar")).toContainText(cs("status.walls", { n: 5, p: 1 }));
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    await status.selectOption("offer");
    await expect(app.toasts().filter({ hasText: "not_calculated" })).toBeVisible({ timeout: 10_000 });
    await expect(status).toHaveValue("draft");
    delay = false;
    await page.waitForTimeout(1500);
    expect((await loadProject(id)).job.status).toBe("draft");
    // The refusal is also logged; nothing else went wrong.
    expect(app.errors.every((e) => e.includes("not_calculated"))).toBe(true);
    app.errors.length = 0;
  });

  test("the note names the figure that differs when the areas are equal (ERR-304)", async ({ app, page, mcp }) => {
    const id = await offered(mcp, "Jiný materiál");
    await withItems(id, (p) => (p.parts[0].edges[0].material = 2));
    await openAfterAi(app, "Jiný materiál");
    await expect(page.locator(".results .hero strong")).not.toHaveText("—");
    await jobTab(page).click();
    await page.locator("aside .job select").selectOption("offer");
    const note = app.toasts().filter({ hasText: cs("job.offerKeptTitle") });
    await expect(note).toBeVisible({ timeout: 10_000 });
    const text = ((await note.textContent()) ?? "").replace(/\s+/g, " ");
    // Not two equal areas: the material whose area changed, with both figures.
    const areas = [...text.matchAll(/(\d+,\d\d) m²/g)].map((m) => m[1]);
    expect(areas.length).toBeGreaterThanOrEqual(2);
    expect(new Set(areas.slice(0, 2)).size).toBe(2);
    expect((await waitForProject(id, (p) => p.job.status === "offer")).job.status).toBe("offer");
  });
});

test.describe("round-4 audit: hand-drawn faces, small windows, the calculating mark", () => {
  const face = (x0: number, x1: number) => ({ points: [{ x: x0, y: 0 }, { x: x1, y: 0 }, { x: x1, y: 4 }, { x: x0, y: 4 }], pitch_deg: 30 });
  /** Whether the first hand-drawn face lies inside the plan's box. */
  const faceInPlan = async (page: Page) => {
    const f = (await page.locator(".manual-face").first().boundingBox())!;
    const p = (await page.locator(".view .wrap > svg").first().boundingBox())!;
    return f.x >= p.x && f.y >= p.y && f.x + f.width <= p.x + p.width && f.y + f.height <= p.y + p.height;
  };

  test("a job of hand-drawn faces only opens for looking, with the face in the plan (UX-301, TEST-305)", async ({ app, page }) => {
    const id = await createExample("gable", "Jen plochy");
    await withItems(id, (p) => ((p.parts = []), (p.dormers = []), (p.chimneys = []), (p.manual_faces = [face(20, 26)])));
    await openEditor(app, "Jen plochy");
    // Not the drawing tool: the first click would start a footprint.
    await expect(tool(page, "tool.draw")).toHaveAttribute("aria-pressed", "false");
    await expect(bar(page, "draw.title")).toHaveCount(0);
    await expect(page.locator(".manual-face")).toHaveCount(1);
    expect(await faceInPlan(page)).toBe(true);
    // No ridge of a roof that has none (UX-302).
    await expect(page.locator(".results .hero strong")).not.toHaveText("—");
    await expect(page.getByTestId("res-ridge")).toHaveCount(0);
    // A click on the face selects it.
    const f = (await page.locator(".manual-face").first().boundingBox())!;
    await page.mouse.click(f.x + f.width / 2, f.y + f.height / 2);
    await expect(bar(page, "manual.face")).toBeVisible();
    expect((await loadProject(id)).parts).toHaveLength(0);
  });

  test("Fit brings a hand-drawn face beside the house into the plan (TEST-305)", async ({ app, page }) => {
    const id = await createExample("gable", "Plocha vedle");
    await withItems(id, (p) => (p.manual_faces = [face(16, 22)]));
    await openEditor(app, "Plocha vedle");
    await expect(page.locator(".manual-face")).toHaveCount(1);
    expect(await faceInPlan(page)).toBe(true);
    // Zoom in so the face leaves the plan, then Fit.
    const p = (await page.locator(".view .wrap > svg").first().boundingBox())!;
    await page.mouse.move(p.x + 40, p.y + p.height / 2);
    for (let i = 0; i < 6; i++) await page.mouse.wheel(0, -400);
    await expect.poll(() => faceInPlan(page)).toBe(false);
    await tool(page, "plan.fit").click();
    await expect.poll(() => faceInPlan(page)).toBe(true);
  });

  test("the empty job's two start buttons are inside the wall bar at 600x420 (UI-303)", async ({ app, page }) => {
    await page.setViewportSize({ width: 600, height: 420 });
    const id = await createExample("gable", "Prázdná lišta");
    await withItems(id, (p) => ((p.parts = []), (p.dormers = []), (p.chimneys = [])));
    await openEditor(app, "Prázdná lišta");
    await page.keyboard.press("Escape");
    const def = bar(page, "def.title");
    const box = (await def.boundingBox())!;
    for (const key of ["tool.draw", "tool.standard"]) {
      const b = (await def.getByRole("button", { name: cs(key) }).boundingBox())!;
      expect(b.x, key).toBeGreaterThanOrEqual(box.x);
      expect(b.x + b.width, key).toBeLessThanOrEqual(Math.min(box.x + box.width, 600));
      const hit = await page.evaluate(([x, y]) => !!document.elementFromPoint(x, y)?.closest("[role=toolbar] button"), [b.x + b.width / 2, b.y + b.height / 2]);
      expect(hit, `${key} can be clicked`).toBe(true);
    }
  });

  test("the calculating mark covers neither view's label nor a view button (UI-309)", async ({ app, page }) => {
    await createExample("gable", "Značka počítání");
    let delay = false;
    await slow(page, "calc", 2500, () => delay);
    await openEditor(app, "Značka počítání");
    await expect(page.locator(".results .hero strong")).not.toHaveText("—");
    await expect(page.locator(".views.split")).toHaveCount(1);
    delay = true;
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), "22");
    await expect(page.getByTestId("calc-pending")).toBeVisible();
    const over = await page.evaluate(() => {
      const m = document.querySelector('[data-testid="calc-pending"]')!.getBoundingClientRect();
      return [...document.querySelectorAll(".views button, .view-label")]
        .filter((e) => {
          const b = e.getBoundingClientRect();
          return b.width > 0 && b.left < m.right && m.left < b.right && b.top < m.bottom && m.top < b.bottom;
        })
        .map((e) => (e.getAttribute("aria-label") || e.textContent || "").trim());
    });
    expect(over).toEqual([]);
    delay = false;
    await expect(page.getByTestId("calc-pending")).toHaveCount(0, { timeout: 10_000 });
  });

  test("an AI settings change made right after the app starts is shown for review, and stays after another is reverted (TEST-307)", async ({ app, page, mcp }) => {
    await app.open();
    await mcp.call("update_settings", { roof_defaults: { pitch_deg: 25 }, summary: "First change." });
    await mcp.call("update_settings", { roof_defaults: { overhang: 0.7 }, summary: "Second change." });
    const card = page.locator("section.card[role=status]");
    await expect(card.filter({ hasText: "Second change." })).toBeVisible();
    await card.getByRole("button", { name: cs("ai.revert") }).first().click();
    // The other open change is the card now, by its own summary.
    await expect(card.filter({ hasText: "First change." })).toBeVisible();
    const s = (await mcp.call("get_settings")).settings;
    expect(s.ai_changes.map((c: any) => [c.summary, c.state])).toEqual([
      ["First change.", "open"],
      ["Second change.", "reverted"],
    ]);
  });
});
