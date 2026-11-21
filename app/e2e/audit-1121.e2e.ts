// Guards for the 2026-11-21 audit's UI and UX findings: double clicks that
// act once (trash, attachments, card menu), localized file sizes, a status
// change without ownership warnings, plan labels in view and off other
// buildings, the 3D view strip at the minimum size, line lengths on one line,
// keyboard rows in the calculation tab, a plan that keeps still or follows the
// drawing, a dormer dialog that warns before OK, Done refused like an offer,
// German toolbar labels, AI wall notes at their wall, the previous-state
// check, Enter for a hand-drawn face and an empty job without zero figures.
import type { Page } from "@playwright/test";
import { clickPlan, openEditor, plan, selectWall, setField, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, scratchFile, test, waitForProject } from "./fixtures";
import { cs, tr } from "./i18n";

const jobTab = (page: Page) => page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") });

/** Label boxes (shrunk by 3 px) that a line crosses, in plan pixels. */
const crossings = (page: Page, labels: string, lines: string) =>
  plan(page).evaluate(
    (svg, [labels, lines]) => {
      const o = svg.getBoundingClientRect();
      const segs = [...svg.querySelectorAll(lines)].map((l) => ["x1", "y1", "x2", "y2"].map((a) => +l.getAttribute(a)!));
      const hit = (s: number[], b: number[]) => {
        let [t0, t1] = [0, 1];
        const [dx, dy] = [s[2] - s[0], s[3] - s[1]];
        for (const [pp, q] of [[-dx, s[0] - b[0]], [dx, b[2] - s[0]], [-dy, s[1] - b[1]], [dy, b[3] - s[1]]]) {
          if (pp === 0) {
            if (q < 0) return false;
          } else if (pp < 0) t0 = Math.max(t0, q / pp);
          else t1 = Math.min(t1, q / pp);
          if (t0 > t1) return false;
        }
        return true;
      };
      const bad: string[] = [];
      for (const el of svg.querySelectorAll(labels)) {
        const r = el.getBoundingClientRect();
        const b = [r.left - o.left + 3, r.top - o.top + 3, r.right - o.left - 3, r.bottom - o.top - 3];
        if (segs.some((sg) => hit(sg, b))) bad.push(el.textContent ?? "");
      }
      return bad;
    },
    [labels, lines] as const,
  );

test.describe("audit 2026-11-21 UI", () => {
  test("a double click on Restore in the trash restores once, with no error, and the delete toast's offer goes (UI-001)", async ({ app, page }) => {
    await createExample("gable", "Koš dvakrát");
    await app.open();
    const row = () => page.getByRole("row").filter({ hasText: "Koš dvakrát" });
    await row().first().click();
    await app.answerConfirm(true);
    await page.locator("aside.detail").getByRole("button", { name: cs("card.delete") }).click();
    await expect(row()).toHaveCount(0);
    const deleted = app.toasts().filter({ hasText: cs("home.deleted", { name: "Koš dvakrát" }) });
    await expect(deleted).toBeVisible();
    await page.getByTestId("lib-trash").click();
    await page.getByTestId("trash-list").getByRole("listitem").filter({ hasText: "Koš dvakrát" }).getByRole("button", { name: cs("home.restore") }).dblclick();
    await expect.poll(async () => (await listProjects()).map((p: any) => p.name)).toContain("Koš dvakrát");
    await expect(app.toasts().filter({ hasText: cs("home.restored", { name: "Koš dvakrát" }) })).toBeVisible();
    await page.waitForTimeout(600);
    await expect(app.toasts().filter({ hasText: cs("err.action.restore") })).toHaveCount(0);
    await expect(deleted).toHaveCount(0);
  });

  test("a double click on an attachment's remove asks once and reports nothing missing; sizes use the decimal comma (UI-009, UI-010)", async ({ app, page }) => {
    await createExample("gable", "Přílohy");
    const big = scratchFile("velký výkres.png", Buffer.alloc(Math.round(1.5 * 1048576), 1));
    await openEditor(app, "Přílohy");
    await jobTab(page).click();
    const job = page.locator("aside .job");
    await app.answerFileDialog(big);
    await job.getByRole("button", { name: cs("files.add") }).click();
    await expect(job.locator("ul.files li")).toHaveCount(1);
    await expect(job.locator("ul.files .size")).toHaveText("1,5 MB");
    await app.answerConfirm(true, true);
    await job.locator("ul.files li").getByRole("button", { name: cs("files.remove") }).dblclick();
    await expect(job.locator("ul.files li")).toHaveCount(0);
    await page.waitForTimeout(600);
    expect((await app.dialogs()).filter((d) => d.kind === "confirm").length).toBe(1);
    await expect(app.toasts().filter({ hasText: cs("err.action.files") })).toHaveCount(0);
  });

  test("a double click on a grid card's More opens only its menu, not the job (UI-010)", async ({ app, page }) => {
    await createExample("gable", "Karta");
    await app.open();
    await page.getByRole("button", { name: cs("lib.grid") }).click();
    const more = page.locator("article.card").first().getByRole("button", { name: cs("card.more") });
    await more.dblclick();
    await page.waitForTimeout(500);
    await expect(page.locator(".editor")).toHaveCount(0);
  });

  test("a status change writes no ownership warning (UI-010, ERRU-004)", async ({ app, page }) => {
    const id = await createExample("gable", "Vlastnictví");
    const warnings: string[] = [];
    page.on("console", (m) => m.type() === "warning" && m.text().includes("ownership_invalid_mutation") && warnings.push(m.text()));
    await openEditor(app, "Vlastnictví");
    await jobTab(page).click();
    await page.locator("aside .job select").selectOption("offer");
    await waitForProject(id, (p) => p.job.status === "offer" && !!p.result_snapshot);
    expect(warnings).toEqual([]);
  });

  test("at 1024 px the side walls' labels lie inside the plan (UI-010)", async ({ app, page }) => {
    await createExample("t-shape", "Téčko");
    await page.setViewportSize({ width: 1024, height: 768 });
    await openEditor(app, "Téčko");
    await expect.poll(() => plan(page).locator(".edge-dim").count()).toBeGreaterThan(4);
    const out = await plan(page).evaluate((svg) => {
      const o = svg.getBoundingClientRect();
      return [...svg.querySelectorAll(".edge-dim")]
        .filter((el) => {
          const r = el.getBoundingClientRect();
          return r.left < o.left || r.right > o.right || r.top < o.top || r.bottom > o.bottom;
        })
        .map((el) => el.textContent);
    });
    expect(out).toEqual([]);
  });

  test("at 600x420 the 3D view's last view button can be reached with the wheel (UI-002)", async ({ app, page }) => {
    await createExample("gable", "Malé okno");
    await openEditor(app, "Malé okno");
    await page.setViewportSize({ width: 600, height: 420 });
    const strip = page.locator(".view .views");
    await expect(strip).toBeVisible();
    const wide = await strip.evaluate((el) => el.scrollWidth > el.clientWidth + 1);
    if (wide) {
      await strip.hover();
      await page.mouse.wheel(0, 300);
      await expect.poll(() => strip.evaluate((el) => el.scrollLeft)).toBeGreaterThan(0);
    }
    const last = strip.getByRole("radio").last();
    const r = (await last.boundingBox())!;
    const s = (await strip.boundingBox())!;
    expect(r.x + r.width).toBeLessThanOrEqual(s.x + s.width + 1);
  });

  test("face labels of a roof keep off a building drawn inside it (UI-005)", async ({ app, page }) => {
    const id = await createExample("gable", "Uvnitř");
    const p = await loadProject(id);
    p.parts.push({
      name: "Vnitřní",
      edges: [
        [4, 2],
        [8, 2],
        [8, 6],
        [4, 6],
      ].map(([x, y]) => ({ start: { x, y }, pitch_deg: 40, eave_height: 3, overhang: 0.5 })),
    });
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Uvnitř");
    await expect.poll(() => plan(page).locator(".face-tag").count()).toBeGreaterThan(0);
    expect(await crossings(page, ".face-tag", "g.edge line.wall")).toEqual([]);
  });

  test("line lengths in Results keep their unit on the same line (UI-006)", async ({ app, page }) => {
    const id = await createExample("gable", "Přístavek");
    const p = await loadProject(id);
    p.parts.push({
      name: "Přístavek",
      edges: [
        [12, 2],
        [16, 2],
        [16, 6],
        [12, 6],
      ].map(([x, y]) => ({ start: { x, y }, pitch_deg: 25, eave_height: 2.5, overhang: 0.5 })),
    });
    await bridge("save_project", { id, project: p });
    await openEditor(app, "Přístavek");
    const cells = page.locator('[data-testid="res-lines"] td.v');
    await expect.poll(() => cells.count()).toBeGreaterThan(3);
    // The text of each cell lies on one line: its rectangles share one top.
    const tops = await cells.evaluateAll((els) =>
      els.map((el) => {
        const r = document.createRange();
        r.selectNodeContents(el);
        return [el.textContent, new Set([...r.getClientRects()].map((q) => Math.round(q.top))).size] as const;
      }),
    );
    for (const [text, n] of tops) expect(n, String(text)).toBe(1);
    await expect(cells.first()).toHaveCSS("white-space", "nowrap");
  });

  test("calculation tab: a line row opens its detail with Enter (UI-008)", async ({ app, page }) => {
    await createExample("gable", "Postup");
    await openEditor(app, "Postup");
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.breakdown") }).click();
    const group = page.locator("details.lines").first();
    await group.locator("summary").click();
    const row = group.locator("tr.clickable").first();
    await expect(row).toHaveAttribute("role", "button");
    await row.focus();
    await expect(row).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog")).toBeVisible();
  });

  test("selecting a wall with the bar's More row open does not move the plan (UI-011)", async ({ app, page }) => {
    await createExample("gable", "Skok");
    await openEditor(app, "Skok");
    const wall = plan(page).locator("g.edge line.wall").first();
    await page.waitForTimeout(400);
    const before = (await wall.boundingBox())!;
    await selectWall(page, 0);
    await page.waitForTimeout(400);
    const after = (await wall.boundingBox())!;
    expect(Math.abs(after.y - before.y)).toBeLessThanOrEqual(1);
    expect(Math.abs(after.x - before.x)).toBeLessThanOrEqual(1);
  });
});

test.describe("audit 2026-11-21 UX", () => {
  test("typed walls keep the drawing in view (UX-001)", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByLabel(cs("job.name")).fill("Stodola");
    await dlg.getByRole("tab", { name: cs("new.draw") }).click();
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    const draw = page.getByRole("toolbar", { name: cs("draw.title") });
    await expect(draw).toBeVisible();
    await clickPlan(page, 2, 2);
    const len = draw.getByLabel(cs("draw.length"));
    await len.fill("24");
    await len.press("ArrowRight");
    await len.fill("12");
    await len.press("ArrowUp");
    await expect(draw).toContainText(cs("draw.points", { n: 3 }));
    await page.waitForTimeout(300);
    const outside = await plan(page).evaluate((svg) => {
      const o = svg.getBoundingClientRect();
      return [...svg.querySelectorAll("circle.draft-pt")].filter((c) => {
        const r = c.getBoundingClientRect();
        return r.left < o.left || r.right > o.right || r.top < o.top || r.bottom > o.bottom;
      }).length;
    });
    expect(outside).toBe(0);
    await expect(len).toBeFocused();
  });

  const openDormerDialog = async (app: import("./fixtures").App, page: Page, name: string, y: number) => {
    const id = await createExample("gable", name);
    await openEditor(app, name);
    await tool(page, "tool.dormer").click();
    // On the lower face, the ridge at y = 4.
    await clickPlan(page, 5, y);
    const dlg = page.getByRole("dialog", { name: cs("dormer.dialogNew") });
    await expect(dlg).toBeVisible();
    return { id, dlg };
  };

  test("a default dormer placed below the ridge fits under it (UX-002)", async ({ app, page }) => {
    const { id, dlg } = await openDormerDialog(app, page, "Vikýř", 2.4);
    await page.waitForTimeout(1200);
    await expect(dlg.getByTestId("dormer-above-roof")).toHaveCount(0);
    await dlg.getByRole("button", { name: cs("common.ok"), exact: true }).click();
    await waitForProject(id, (p) => p.dormers.length === 1);
    await page.waitForTimeout(800);
    await expect(page.locator(".results .warn")).toHaveCount(0);
  });

  test("a dormer raised above the ridge is marked in the dialog before OK (UX-002)", async ({ app, page }) => {
    const { dlg } = await openDormerDialog(app, page, "Vikýř 2", 1.6);
    await setField(dlg.getByLabel(cs("dormer.wallHeight")), 3);
    await expect(dlg.getByTestId("dormer-above-roof")).toBeVisible();
    await setField(dlg.getByLabel(cs("dormer.wallHeight")), 0.5);
    await expect(dlg.getByTestId("dormer-above-roof")).toHaveCount(0);
  });

  test("with the roof not calculated Done is refused like Offer and Ordered (UX-005)", async ({ app, page }) => {
    const id = await createExample("hip", "Bez střechy");
    await page.route(/\/calc$/, (r) =>
      r.fulfill({ contentType: "application/json", headers: { "access-control-allow-origin": "*" }, body: JSON.stringify({ err: { code: "too_many", detail: "too many pitch breaks of one wall: 65 (at most 64)" } }) }),
    );
    await openEditor(app, "Bez střechy");
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    for (const s of ["offer", "ordered", "done"]) await expect(status.locator(`option[value="${s}"]`)).toHaveAttribute("disabled", "");
    await expect(app.toasts().filter({ hasText: cs("err.action.save") })).toHaveCount(0);
    expect((await loadProject(id)).job.status).toBe("draft");
    app.errors.length = 0;
  });

  test("a refused status says so, not that the job was not saved (UX-005)", async ({ app, page }) => {
    const id = await createExample("hip", "Bez snímku");
    await openEditor(app, "Bez snímku");
    await page.route(/\/take_snapshot$/, (r) =>
      r.fulfill({ contentType: "application/json", headers: { "access-control-allow-origin": "*" }, body: JSON.stringify({ err: { code: "not_calculated", detail: "no roof" } }) }),
    );
    await jobTab(page).click();
    const status = page.locator("aside .job select");
    await status.selectOption("done");
    await expect(app.toasts().filter({ hasText: cs("err.action.status") })).toBeVisible();
    await expect(app.toasts().filter({ hasText: cs("err.action.save") })).toHaveCount(0);
    await expect(status).toHaveValue("draft");
    expect((await loadProject(id)).job.status).toBe("draft");
    app.errors.length = 0;
  });

  for (const [w, h] of [
    [1360, 860],
    [1440, 900],
  ]) {
    test(`German toolbar shows the tool names at ${w}x${h} (UX-006)`, async ({ app, page }) => {
      await createExample("gable", "Satteldach");
      await page.setViewportSize({ width: w, height: h });
      await app.open({ locale: "de" });
      await app.openJob("Satteldach");
      const tb = page.getByRole("toolbar").first();
      await page.waitForTimeout(500);
      for (const key of ["tool.standard", "tool.face", "tool.flat", "tool.draw"]) {
        const label = tb.getByRole("button", { name: tr("de", key), exact: true }).locator("span");
        await expect(label).toBeVisible();
        expect((await label.boundingBox())!.width, key).toBeGreaterThan(8);
      }
    });
  }

  test("an AI note for a side wall stands at that wall (UX-007)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba AI");
    await openEditor(app, "Valba AI");
    await mcp.call("update_edges", { id, changes: [{ index: 1, overhang: 0.8 }], summary: "Přesah podle řezu" });
    const note = plan(page).getByTestId("wall-note");
    await expect(note).toHaveCount(1);
    const wall = plan(page).locator("g.edge line.wall").nth(1);
    const n = (await note.boundingBox())!;
    const w = (await wall.boundingBox())!;
    // Wall 2 is the right side (x = 12): the note's centre lies within 40 px of it.
    expect(Math.abs(n.x + n.width / 2 - (w.x + w.width / 2))).toBeLessThan(40);
  });

  test("the previous-state button is a check that says what it shows (UX-008)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba před");
    await openEditor(app, "Valba před");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 25 }], summary: "Sklon podle řezu" });
    const before = page.getByTestId("review-before");
    await expect(before).toHaveAccessibleName(cs("review.before"));
    expect(cs("review.before")).toMatch(/^Ukázat/);
    await expect(before).toHaveAttribute("aria-pressed", "true");
    await expect(before.locator(".chk svg")).toHaveCount(1);
    await before.click();
    await expect(before).toHaveAttribute("aria-pressed", "false");
    await expect(before.locator(".chk svg")).toHaveCount(0);
    await expect(page.locator(".ghost line")).toHaveCount(0);
  });

  test("Enter closes a hand-drawn face (UX-010)", async ({ app, page }) => {
    const id = await createExample("gable", "Ruční");
    await openEditor(app, "Ruční");
    await tool(page, "tool.face").click();
    const pb = page.getByTestId("place-bar");
    await expect(pb.getByTestId("place-steps")).toHaveText(cs("plan.placeFace"));
    for (const [x, y] of [
      [2, 1],
      [6, 1],
      [6, 3],
      [2, 3],
    ])
      await clickPlan(page, x, y);
    await expect(pb.getByTestId("place-points")).toHaveText(cs("place.points", { n: 4 }));
    await page.keyboard.press("Enter");
    await waitForProject(id, (p) => (p.manual_faces ?? []).length === 1);
    await expect(pb).toHaveCount(0);
  });

  test("an empty job shows a dash and a hint, not 0,00 (UX-011)", async ({ app, page }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByLabel(cs("job.name")).fill("Prázdná");
    await dlg.getByRole("tab", { name: cs("new.draw") }).click();
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    const results = page.locator(".results");
    await expect(results.getByTestId("res-area")).toHaveText("—");
    await expect(results.getByTestId("res-empty")).toHaveText(cs("res.empty"));
    await expect(results).not.toContainText("0,00 m");
  });
});
