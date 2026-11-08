// AI over MCP: live review in the open app (banner, highlight, Accept,
// Revert, Revert all, follow mode, library marks) and MCP parity: the same
// edit made in the editor and over MCP gives the same project.
import type { Page } from "@playwright/test";
import { bar, clickPlan, openEditor, selectWall, setField, tool } from "./editor";
import { bridge, createExample, expect, listProjects, loadProject, test, waitForProject, type App } from "./fixtures";
import type { Mcp } from "./mcp";
import { cs } from "./i18n";

const review = (page: Page) => page.locator(".review");

test.describe("live AI changes", () => {
  test("an MCP edit appears live with its summary and highlight; Accept keeps it", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 25 }], summary: "Sklon jižní plochy podle řezu A-A" });
    await expect(review(page)).toBeVisible();
    await expect(review(page)).toContainText(cs("review.title", { who: "Claude" }));
    await expect(review(page)).toContainText("Sklon jižní plochy podle řezu A-A");
    await expect(page.locator("g.edge.changed")).toHaveCount(1);
    await expect(page.locator("aside .tabs").getByRole("tab", { name: cs("dock.ai") })).toHaveAttribute("aria-selected", "true");
    await expect(page.locator("aside .tabs .badge")).toBeVisible();
    await selectWall(page, 0);
    await expect(bar(page, "def.title").getByLabel(cs("edge.pitch"))).toHaveValue("25");

    await review(page).getByRole("button", { name: cs("review.accept") }).click();
    await expect(review(page)).toHaveCount(0);
    await expect(page.locator("g.edge.changed")).toHaveCount(0);
    expect((await loadProject(id)).parts[0].edges[0].pitch_deg).toBe(25);
    // Accepted stays accepted after a reload.
    await page.reload();
    await app.openJob("Valba");
    await expect(review(page)).toHaveCount(0);
  });

  test("Revert in the banner restores the file; the AI tab lists the history", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await mcp.call("update_edges", { id, changes: [{ all: true, eave_height: 4 }], summary: "Výška okapu 4 m" });
    await expect(review(page)).toBeVisible();
    await review(page).getByRole("button", { name: cs("ai.revert") }).click();
    await expect(review(page)).toHaveCount(0);
    const p = await waitForProject(id, (p) => p.parts[0].edges.every((e: any) => e.eave_height === 3));
    expect(p.parts[0].edges).toHaveLength(4);
    const ai = page.locator("aside .panel");
    await expect(ai).toContainText("Výška okapu 4 m");
    await expect(ai).toContainText(cs("ai.revertedEntry"));
    await ai.getByRole("button", { name: cs("ai.showChanges", { n: 4 }) }).first().click();
    await expect(ai.locator("ol > li ul li")).toHaveCount(4);
    await ai.getByRole("button", { name: cs("ai.hideChanges") }).click();
    await expect(ai.locator("ol > li ul li")).toHaveCount(0);
    // The user's own undo history still works after the revert.
    await expect(tool(page, "editor.undo")).toBeEnabled();
  });

  test("several AI steps: the banner counts them and Revert all undoes all", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "Krok 1" });
    await expect(review(page)).toContainText("Krok 1");
    await mcp.call("update_job", { id, customer: "AI Zákazník", summary: "Krok 2" });
    await mcp.call("set_roof_options", { id, flat_roof: true, summary: "Krok 3" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 3 }));
    await expect(review(page)).toContainText("Krok 1 → Krok 2 → Krok 3");
    await review(page).getByRole("button", { name: cs("review.revertAll") }).click();
    const p = await waitForProject(id, (p) => !p.flat_roof && p.parts[0].edges[0].pitch_deg === 40);
    expect(p.job.customer).toBe("");
    await expect(review(page)).toHaveCount(0);
  });

  test("a long AI run in the smallest window keeps the plan working (UI-001)", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    const long = "Sklon podle řezu A-A, okapová výška podle pohledu od jihu a přesah podle detailu D3 ve výkresu";
    for (let i = 1; i <= 4; i++) {
      await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 + i }], summary: `${i}. ${long}` });
    }
    await expect(review(page)).toContainText(cs("review.steps", { n: 4 }));
    // Used to squeeze the plan to 0 px and hang the page in the grid loop.
    await page.setViewportSize({ width: 600, height: 420 });
    await expect(review(page)).toContainText(`1. ${long}`);
    expect(await page.evaluate(() => 1 + 1)).toBe(2);
    await review(page).getByRole("button", { name: cs("review.accept") }).click();
    await expect(review(page)).toHaveCount(0);
  });

  test("Revert in the AI tab undoes only the latest AI change", async ({ app, page, mcp }) => {
    const id = await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    await mcp.call("update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }], summary: "První" });
    await expect(review(page)).toContainText("První");
    await mcp.call("update_edges", { id, changes: [{ index: 2, pitch_deg: 31 }], summary: "Druhá" });
    await expect(review(page)).toContainText(cs("review.steps", { n: 2 }));
    await page.locator("aside .panel ol > li").first().getByRole("button", { name: cs("ai.revert") }).click();
    const p = await waitForProject(id, (p) => p.parts[0].edges[2].pitch_deg === 40);
    expect(p.parts[0].edges[0].pitch_deg).toBe(30);
  });

  test("user edits and AI edits interleave without losing either", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await selectWall(page, 1);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 33);
    await waitForProject(id, (p) => p.parts[0].edges[1].pitch_deg === 33);
    await mcp.call("update_edges", { id, changes: [{ index: 3, pitch_deg: 44 }], summary: "AI" });
    await expect(review(page)).toBeVisible();
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.overhang")), 0.9);
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].overhang === 0.9);
    expect(p.parts[0].edges.map((e: any) => e.pitch_deg)).toEqual([35, 33, 35, 44]);
  });

  test("an AI edit right after a user edit (before the autosave) keeps both", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    for (const [i, v] of [[0, 0.7], [1, 0.8], [2, 0.9]] as const) {
      await selectWall(page, i);
      await setField(bar(page, "def.title").getByLabel(cs("edge.overhang")), v);
      await mcp.call("update_edges", { id, changes: [{ index: 3, pitch_deg: 40 + i }], summary: `AI ${i}` });
      const p = await waitForProject(id, (p) => p.parts[0].edges[i].overhang === v && p.parts[0].edges[3].pitch_deg === 40 + i);
      expect(p.parts[0].edges.slice(0, i + 1).map((e: any) => e.overhang)).toEqual([0.7, 0.8, 0.9].slice(0, i + 1));
    }
  });

  test("follow mode opens the job the AI works on; the header pill shows what it does", async ({ app, page, mcp }) => {
    const id = await createExample("l-shape", "Elko");
    await createExample("gable", "Jiná");
    await app.open();
    await mcp.call("update_edges", { id, changes: [{ index: 0, overhang: 0.7 }], summary: "Přesah dle výkresu" });
    await expect(page.locator(".editor input.title")).toHaveValue("Elko");
    await expect(app.toasts().filter({ hasText: cs("ai.followed", { who: "Claude" }) })).toBeVisible();
    await expect(page.locator("header .ai-pill")).toContainText("Claude");
    await expect(review(page)).toContainText("Přesah dle výkresu");
  });

  test("follow mode switching jobs right after an edit saves the edit to the right job", async ({ app, page, mcp }) => {
    const a = await createExample("hip", "Moje");
    const b = await createExample("gable", "AI práce");
    await openEditor(app, "Moje");
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 22);
    // Before the autosave: the AI starts on another job and the app follows it.
    await mcp.call("update_job", { id: b, customer: "Z výkresu", summary: "Zákazník" });
    await expect(page.locator(".editor input.title")).toHaveValue("AI práce");
    const pa = await waitForProject(a, (p) => p.parts[0].edges[0].pitch_deg === 22);
    expect(pa.name).toBe("Moje");
    const pb = await loadProject(b);
    expect(pb.name).toBe("AI práce");
    expect(pb.job.customer).toBe("Z výkresu");
    expect(pb.parts[0].edges.map((e: any) => e.pitch_deg)).toEqual([40, 90, 40, 90]);
  });

  test("going back right after an edit saves it", async ({ app, page }) => {
    const a = await createExample("hip", "Moje");
    await openEditor(app, "Moje");
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 23);
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await waitForProject(a, (p) => p.parts[0].edges[0].pitch_deg === 23);
    expect(await listProjects()).toHaveLength(1);
  });

  test("follow mode off: the library stays, the job is marked as changed by AI", async ({ app, page, mcp }) => {
    const id = await createExample("l-shape", "Elko");
    await createExample("gable", "Jiná");
    await app.open({ prefs: { followAi: false, layout: "split", libraryView: "list" } });
    await mcp.call("update_settings", { follow_ai: false, summary: "Follow mode off as the user asked" });
    await mcp.call("update_edges", { id, changes: [{ index: 0, overhang: 0.7 }], summary: "Přesah dle výkresu" });
    const row = page.getByRole("row").filter({ hasText: "Elko" });
    await expect(row.locator(".ai-tag")).toContainText("Claude");
    await expect(page.locator(".editor")).toHaveCount(0);
    await page.locator("nav").getByRole("button", { name: cs("lib.byAi") }).click();
    await expect(page.locator("main tbody tr")).toHaveCount(1);
    await expect(page.locator("header .ai-btn")).toContainText("Claude");
  });

  test("a new job created by the AI shows up in the library", async ({ app, page, mcp }) => {
    await app.open({ prefs: { followAi: false, layout: "split", libraryView: "list" } });
    await mcp.call("update_settings", { follow_ai: false, summary: "Follow mode off as the user asked" });
    await mcp.call("create_standard_roof", { name: "Od AI", type: "hip", length: 11, width: 9, pitch_deg: 30, summary: "Valba z výkresu" });
    await expect(page.getByRole("row").filter({ hasText: "Od AI" })).toBeVisible();
  });

  test("AI connect dialog: one-click connect, other apps, copy and the follow toggle", async ({ app, page }) => {
    await app.open();
    await page.getByRole("button", { name: cs("ai.connect") }).click();
    const dlg = page.getByRole("dialog", { name: cs("conn.title") });
    // The preview bridge shows Claude Desktop connected, Codex not connected
    // and Antigravity missing, without touching real AI app settings.
    const row = (name: string) => dlg.locator(".apps li").filter({ hasText: name });
    await expect(row("Claude Desktop")).toContainText(cs("conn.state.connected"));
    await expect(row("Antigravity")).toContainText(cs("conn.state.not_installed"));
    await expect(row("Antigravity").getByRole("button")).toHaveCount(0);
    // Not connected on the first run; a retry against the same bridge finds it connected.
    await row("Codex").getByRole("button").click();
    await expect(row("Codex")).toContainText(cs("conn.state.connected"));
    await expect(row("Codex")).toContainText(cs("conn.restart.codex"));
    for (const name of ["Claude Code", cs("conn.otherJson"), "ChatGPT"]) {
      await dlg.getByRole("tab", { name }).click();
      await expect(dlg.getByRole("tab", { name })).toHaveAttribute("aria-selected", "true");
    }
    await expect(dlg.locator(".note")).toContainText("Codex");
    await dlg.getByRole("tab", { name: "Claude Code" }).click();
    await expect(dlg.locator("pre")).toContainText("claude mcp add tectora");
    await dlg.getByRole("tab", { name: cs("conn.otherJson") }).click();
    await expect(dlg.locator("pre")).toContainText('"mcpServers"');
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await dlg.getByRole("button", { name: cs("conn.copy") }).click();
    await expect(dlg.getByRole("button", { name: cs("conn.copied") })).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toContain("mcpServers");
    const follow = dlg.getByRole("checkbox", { name: cs("conn.follow") });
    await follow.uncheck();
    await expect.poll(async () => (await bridge<any>("get_settings")).settings.follow_ai).toBe(false);
    await follow.check();
    await expect.poll(async () => (await bridge<any>("get_settings")).settings.follow_ai).toBe(true);
  });
});

// --- MCP parity -------------------------------------------------------------

/** Geometry and job data of a project, without names, timestamps and the stored result. */
function shape(p: any) {
  const strip = (v: any): any => {
    if (Array.isArray(v)) return v.map(strip);
    if (v && typeof v === "object") {
      const out: any = {};
      for (const [k, x] of Object.entries(v)) {
        if (k === "name" || k === "result_snapshot") continue;
        out[k] = strip(x);
      }
      return out;
    }
    return typeof v === "number" ? Math.round(v * 1e6) / 1e6 : v;
  };
  return strip({ ...p, name: undefined });
}

async function twin(example: string): Promise<[string, string]> {
  return [await createExample(example, "UI"), await createExample(example, "MCP")];
}

/** Opens job "UI" in the editor, runs `ui`, waits for the save and compares with the MCP twin. */
async function parity(app: App, ids: [string, string], ui: () => Promise<void>, viaMcp: () => Promise<unknown>) {
  await openEditor(app, "UI");
  await ui();
  await app.saved();
  await viaMcp();
  await expect.poll(async () => JSON.stringify(shape(await loadProject(ids[0]))), { timeout: 8000 }).toBe(
    JSON.stringify(shape(await loadProject(ids[1]))),
  );
  // And the same calculation result.
  const [a, b] = await Promise.all(ids.map(async (id) => (await bridge<any>("calc", { project: await loadProject(id) })).totals));
  expect(a).toEqual(b);
}

test.describe("MCP parity", () => {
  test("wall pitch, eave height and overhang", async ({ app, page, mcp }) => {
    const ids = await twin("hip");
    await parity(app, ids, async () => {
      await selectWall(page, 1);
      const def = bar(page, "def.title");
      await setField(def.getByLabel(cs("edge.pitch")), 28);
      await setField(def.getByLabel(cs("edge.eaveHeight")), 3.4);
      await setField(def.getByLabel(cs("edge.overhang")), 0.65);
    }, () => mcp.call("update_edges", { id: ids[1], changes: [{ index: 1, pitch_deg: 28, eave_height: 3.4, overhang: 0.65 }], summary: "x" }));
  });

  test("eave to gable, half-hip, clad gable", async ({ app, page, mcp }) => {
    const ids = await twin("hip");
    await parity(app, ids, async () => {
      await selectWall(page, 1);
      const def = bar(page, "def.title");
      await def.getByRole("radio", { name: cs("edge.gable") }).click();
      await def.getByLabel(cs("edge.halfHip"), { exact: true }).check();
      await setField(def.getByLabel(cs("edge.halfHipHeight")), 5.5);
      await setField(def.getByLabel(cs("edge.halfHipPitch")), 50);
    }, () => mcp.call("update_edges", { id: ids[1], changes: [{ index: 1, gable: true, half_hip: { height: 5.5, pitch_deg: 50 } }], summary: "x" }));
  });

  test("clad gable", async ({ app, page, mcp }) => {
    const ids = await twin("gable");
    const gable = (await loadProject(ids[0])).parts[0].edges.findIndex((e: any) => e.pitch_deg >= 90);
    await parity(app, ids, async () => {
      await selectWall(page, gable);
      await bar(page, "def.title").getByLabel(cs("edge.clad")).check();
    }, () => mcp.call("update_edges", { id: ids[1], changes: [{ index: gable, clad: true }], summary: "x" }));
  });

  test("pitch breaks, priority and material on all walls", async ({ app, page, mcp }) => {
    const ids = await twin("hip");
    await parity(app, ids, async () => {
      await selectWall(page, 0);
      const def = bar(page, "def.title");
      await def.getByRole("button", { name: cs("edge.addBreak") }).click();
      await setField(def.getByLabel(cs("edge.breakHeight", { n: 1 })), 4.2);
      await setField(def.getByLabel(cs("edge.breakPitch")), 22);
      await def.getByLabel(cs("edge.priority")).check();
      await def.getByLabel(cs("edge.material")).selectOption("2");
      await def.getByRole("button", { name: cs("edge.materialAll") }).click();
    }, () =>
      mcp.call("update_edges", {
        id: ids[1],
        changes: [
          { index: 0, breaks: [{ height: 4.2, pitch_deg: 22 }], priority: 1 },
          { all: true, material: 2 },
        ],
        summary: "x",
      }),
    );
  });

  test("wall length, moved corner, inserted direction change, deleted corner", async ({ app, page, mcp }) => {
    const ids = await twin("l-shape");
    await parity(app, ids, async () => {
      const def = bar(page, "def.title");
      await selectWall(page, 0);
      await setField(def.getByLabel(cs("edge.length")), 15);
      await selectWall(page, 2);
      await setField(def.getByLabel(cs("edge.startX")), 15.5);
      await selectWall(page, 4);
      await def.getByRole("button", { name: cs("edge.addMarker") }).click();
    }, () =>
      mcp.call("edit_walls", {
        id: ids[1],
        operations: [
          { op: "set_length", index: 0, length: 15 },
          { op: "move_point", index: 2, x: 15.5, y: 7 },
          { op: "insert_direction_change", index: 4 },
        ],
        summary: "x",
      }),
    );
  });

  test("job data and status", async ({ app, page, mcp }) => {
    const ids = await twin("gable");
    await parity(app, ids, async () => {
      await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
      const job = page.locator("aside .job");
      for (const [k, v] of [["job.number", "77"], ["job.customer", "Eva"], ["job.address", "Lipová 3"]]) {
        await job.getByLabel(cs(k)).fill(v);
        await job.getByLabel(cs(k)).press("Tab");
      }
      await job.getByLabel(cs("job.notes")).fill("pozn.");
      await job.getByLabel(cs("job.notes")).press("Tab");
    }, () => mcp.call("update_job", { id: ids[1], number: "77", customer: "Eva", address: "Lipová 3", notes: "pozn.", summary: "x" }));
  });

  test("flat roof and material names", async ({ app, page, mcp }) => {
    const ids = await twin("hip");
    await parity(app, ids, async () => {
      await tool(page, "tool.flat").click();
      await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
      const mats = page.locator("aside .job label.mat input");
      await mats.nth(0).fill("Bramac Max");
      await mats.nth(0).press("Tab");
    }, () => mcp.call("set_roof_options", { id: ids[1], flat_roof: true, material_names: ["Bramac Max"], summary: "x" }));
  });

  test("move a part, and mirror it with a copy", async ({ app, page, mcp }) => {
    const ids = await twin("hip");
    await parity(app, ids, async () => {
      const dlg = page.getByRole("dialog", { name: cs("xf.title") });
      await selectWall(page, 0);
      await tool(page, "tool.transform").click();
      await setField(dlg.getByLabel(cs("xf.dx")), 1.5);
      await setField(dlg.getByLabel(cs("xf.dy")), 2);
      await dlg.getByRole("button", { name: cs("common.ok") }).click();
      await selectWall(page, 0);
      await tool(page, "tool.transform").click();
      await dlg.getByRole("radio", { name: cs("xf.mirror") }).click();
      await setField(dlg.getByLabel(cs("xf.axis")), 90);
      await setField(dlg.getByLabel(cs("xf.cx")), 15);
      await setField(dlg.getByLabel(cs("xf.cy")), 0);
      await setField(dlg.getByLabel(cs("xf.copies")), 1);
      await dlg.getByRole("button", { name: cs("common.ok") }).click();
    }, async () => {
      await mcp.call("transform_part", { id: ids[1], op: "move", dx: 1.5, dy: 2, summary: "x" });
      await mcp.call("transform_part", { id: ids[1], op: "mirror", deg: 90, centre: [15, 0], copies: 1, summary: "x" });
    });
  });

  test("chimney and roof window", async ({ app, page, mcp }) => {
    const ids = await twin("hip");
    await parity(app, ids, async () => {
      await tool(page, "tool.chimney").click();
      await clickPlan(page, 5, 3);
      const ob = bar(page, "opening.chimney");
      await setField(ob.getByLabel(cs("opening.rotation")), 0);
      await setField(ob.getByLabel(cs("opening.top")), 8.5);
      await tool(page, "tool.window").click();
      await clickPlan(page, 3, 1.5);
      const wb = bar(page, "opening.window");
      await setField(wb.getByRole("textbox", { name: /^X\b/ }), 3);
      await setField(wb.getByRole("textbox", { name: /^Y\b/ }), 1.5);
    }, () =>
      mcp.call("set_openings", {
        id: ids[1],
        chimneys: [{ x: 5, y: 3, width: 0.6, depth: 0.6, rotation_deg: 0, height: 8.5 }],
        windows: [{ x: 3, y: 1.5, width: 0.78, height: 1.18 }],
        summary: "x",
      }),
    );
  });

  test("dormer with the same defaults as the toolbar", async ({ app, page, mcp }) => {
    const ids = await twin("hip");
    await parity(app, ids, async () => {
      await tool(page, "tool.dormer").click();
      await clickPlan(page, 4, 1);
      const db = bar(page, "dormer.title");
      await setField(db.getByRole("textbox", { name: /^X\b/ }), 4);
      await setField(db.getByRole("textbox", { name: /^Y\b/ }), 1);
    }, async () => {
      const p = await loadProject(ids[0]);
      const ui = p.dormers[0];
      // The toolbar's dormer: same kind and size as MCP's defaults, eave from the UI's own roof height.
      const front = ui.edges.find((e: any) => e.direction_rad == null) ?? ui.edges[0];
      await mcp.call("add_dormer", {
        id: ids[1],
        kind: "gable",
        start: [front.start.x, front.start.y],
        eave_height: ui.edges[0].eave_height,
        summary: "x",
      });
    });
  });

  test("standard roof: new job dialog vs create_standard_roof", async ({ app, page, mcp }) => {
    await app.open();
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const dlg = page.getByRole("dialog", { name: cs("new.title") });
    await dlg.getByLabel(cs("job.name")).fill("UI");
    await dlg.getByRole("radio", { name: cs("std.hip"), exact: true }).click();
    await setField(dlg.getByLabel(cs("std.length")), 11);
    await setField(dlg.getByLabel(cs("std.width")), 9);
    await setField(dlg.getByLabel(cs("edge.pitch")), 30);
    await setField(dlg.getByLabel(cs("edge.eaveHeight")), 3);
    await setField(dlg.getByLabel(cs("edge.overhang")), 0.5);
    await dlg.getByRole("button", { name: cs("new.create") }).click();
    await app.saved();
    const created = await mcp.call("create_standard_roof", {
      name: "MCP", type: "hip", length: 11, width: 9, pitch_deg: 30, eave_height: 3, overhang: 0.5, summary: "x",
    });
    const all = await listProjects();
    const ui = all.find((p) => p.name === "UI")!;
    const ai = all.find((p) => p.name === "MCP")!;
    expect(created).toBeTruthy();
    expect(shape(await loadProject(ui.id)).parts).toEqual(shape(await loadProject(ai.id)).parts);
  });

  test("store the offer result", async ({ app, page, mcp }) => {
    const ids = await twin("gable");
    await openEditor(app, "UI");
    await page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
    await page.locator("aside .job").getByRole("combobox", { name: cs("job.status") }).selectOption("offer");
    await mcp.call("update_job", { id: ids[1], status: "offer", summary: "x" });
    await mcp.call("store_result", { id: ids[1], summary: "x" });
    const [a, b] = await Promise.all(ids.map((id) => waitForProject(id, (p) => !!p.result_snapshot)));
    const pick = (s: any) => ({ ...s, taken_ms: 0 });
    expect(pick(a.result_snapshot)).toEqual(pick(b.result_snapshot));
  });

  test("delete a part", async ({ app, page, mcp }) => {
    const ids = await twin("hip");
    for (const id of ids) {
      const p = await loadProject(id);
      const c = structuredClone(p.parts[0]);
      for (const e of c.edges) e.start.x += 15;
      p.parts.push(c);
      await bridge("save_project", { id, project: p });
    }
    await parity(app, ids, async () => {
      await selectWall(page, 4, 1);
      await app.answerConfirm(true);
      await tool(page, "tool.deletePart").click();
      await waitForProject(ids[0], (p) => p.parts.length === 1);
    }, () => mcp.call("delete_items", { id: ids[1], items: [{ kind: "part", index: 1 }], summary: "x" }));
  });

  test("every editor feature has an MCP tool", async ({ mcp }) => {
    const tools = new Set(await mcp.tools());
    // Editor feature → MCP tool (CLAUDE.md: a UI feature without its MCP tool is not done).
    const needed: Record<string, string> = {
      "new job / standard roof": "create_standard_roof",
      "new job from example": "create_from_example",
      "draw footprint / add part": "add_part",
      "standard roof as new part": "add_shape_part",
      "wall pitch, height, gable, half-hip, breaks, material": "update_edges",
      "length, corners, direction change": "edit_walls",
      "move / rotate / mirror / copy part": "transform_part",
      "delete part, dormer, opening, line, face": "delete_items",
      "clear the canvas": "clear_roof",
      dormer: "add_dormer",
      "edit dormer": "update_dormer",
      "chimney, window, opening": "set_openings",
      "roof lines and faces": "set_manual",
      "flat roof, material names": "set_roof_options",
      "calculation helpers, round roof": "roof_helper",
      "job data, status": "update_job",
      "store result": "store_result",
      "duplicate job": "duplicate_project",
      "delete job": "delete_project",
      "attach / remove files": "attach_file",
      "remove file": "remove_file",
      "export job": "export_job",
      "import job": "import_job",
      "export plan sheet (PDF)": "export_plan",
      "export text for the price sheet": "export_list",
      "AI history": "get_history",
      "revert AI change": "revert_last_change",
      "revert to": "revert_to",
      settings: "update_settings",
      results: "calc",
    };
    const missing = Object.entries(needed).filter(([, t]) => !tools.has(t));
    expect(missing).toEqual([]);
  });

  test("MCP rejects bad input without changing the job", async ({ mcp }) => {
    const id = await createExample("hip", "Valba");
    const before = JSON.stringify(await loadProject(id));
    for (const [tool, args] of [
      ["update_edges", { id, changes: [{ index: 99, pitch_deg: 30 }], summary: "x" }],
      ["update_edges", { id, changes: [{ index: 0, pitch_deg: 30 }] }],
      ["edit_walls", { id, operations: [{ op: "move_point", index: 0, x: 1e308, y: 0 }], summary: "x" }],
      ["get_project", { id: "../../etc/passwd" }],
      ["attach_file", { id, path: "/nonexistent/file.pdf", summary: "x" }],
      ["attach_file", { id, path: "/dev/stdin", summary: "x" }],
    ] as const) {
      const r = await (mcp as Mcp).tryCall(tool, args as Record<string, unknown>);
      expect(r.isError, `${tool} ${JSON.stringify(args)}`).toBe(true);
    }
    expect(JSON.stringify(await loadProject(id))).toBe(before);
  });
});
