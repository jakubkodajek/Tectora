// Edge-cases class 1 for text fields (TEST-009): the Job tab fields and the
// job title. Escape restores and stores nothing (UI-001), text typed while an
// AI save arrives is kept and stored (UI-002), an empty job name is refused
// in both places (UI-022), and leaving right after typing stores the text.
import type { Locator, Page } from "@playwright/test";
import { bar, openEditor, selectWall } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject, type App } from "./fixtures";
import { cs } from "./i18n";

async function typeInto(page: Page, field: Locator, text: string) {
  await field.click();
  await field.press("ControlOrMeta+a");
  await page.keyboard.type(text);
}

async function openJobTab(app: App, name: string) {
  await openEditor(app, name);
  await app.page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") }).click();
  return app.page.locator("aside .job");
}

/** The Job tab's text fields and where each is stored. */
const FIELDS: [string, (p: any) => string | undefined][] = [
  ["job.name", (p) => p.name],
  ["job.number", (p) => p.job?.number ?? ""],
  ["job.customer", (p) => p.job?.customer ?? ""],
  ["job.address", (p) => p.job?.address ?? ""],
  ["job.notes", (p) => p.notes ?? ""],
];

test.describe("text fields", () => {
  test("Escape in a Job tab field or the title restores it and stores nothing", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    const job = await openJobTab(app, "Valba");
    const before = await loadProject(id);
    for (const [key, get] of FIELDS) {
      const f = job.getByLabel(cs(key));
      const was = await f.inputValue();
      await typeInto(page, f, "Překlep");
      await f.press("Escape");
      await expect(f, key).toHaveValue(was);
      await expect(f, key).not.toBeFocused();
      // A click elsewhere afterwards stores nothing either.
      await page.locator(".view .wrap > svg").first().click({ position: { x: 5, y: 5 } });
      await page.waitForTimeout(700);
      expect(get(await loadProject(id)), key).toBe(get(before));
    }
    const title = page.locator(".editor input.title");
    await typeInto(page, title, "Přejmenováno");
    await title.press("Escape");
    await expect(title).toHaveValue("Valba");
    await page.waitForTimeout(700);
    expect((await loadProject(id)).name).toBe("Valba");
  });

  test("Escape in the label of a hand-drawn line or face restores it and stores nothing (UI-003)", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo");
    const p0 = await loadProject(id);
    p0.manual_lines = [{ code: "T", a: [2, 2, 3], b: [8, 2, 3], label: "Okap dvůr" }];
    p0.manual_faces = [{ points: [{ x: 14, y: 1 }, { x: 17, y: 1 }, { x: 17, y: 5 }, { x: 14, y: 5 }], pitch_deg: 30, label: "Přístavek" }];
    await bridge("save_project", { id, project: p0 });
    await openEditor(app, "Sedlo");
    for (const [kind, select, was] of [
      ["manual.line", () => page.locator("g.manual-line line.hit").first(), "Okap dvůr"],
      ["manual.face", () => page.locator("path.manual-face").first(), "Přístavek"],
    ] as const) {
      await select().dispatchEvent("pointerdown", { button: 0 });
      const mb = bar(page, kind);
      const label = mb.getByLabel(cs("manual.label"));
      await expect(label, kind).toHaveValue(was);
      await typeInto(page, label, "ESC-TEST");
      await label.press("Escape");
      await expect(label, kind).toHaveValue(was);
      await expect(label, kind).not.toBeFocused();
      await page.locator(".view .wrap > svg").first().click({ position: { x: 5, y: 5 } });
      await page.waitForTimeout(700);
      const p = await loadProject(id);
      expect(p.manual_lines[0].label).toBe("Okap dvůr");
      expect(p.manual_faces[0].label).toBe("Přístavek");
    }
  });

  test("text typed in the Job tab while an AI save arrives is kept and stored", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba");
    const job = await openJobTab(app, "Valba");
    const customer = job.getByLabel(cs("job.customer"));
    await typeInto(page, customer, "Novák");
    await mcp.call("update_job", { id, address: "Polní 5", summary: "Adresa z výkresu" });
    await waitForProject(id, (p) => p.job.address === "Polní 5");
    // The review must not switch the tab away under the field.
    await expect(page.locator(".review")).toBeVisible();
    await expect(customer).toBeFocused();
    await expect(customer).toHaveValue("Novák");
    await expect(job.getByLabel(cs("job.address"))).toHaveValue("Polní 5");
    await customer.press("Tab");
    const p = await waitForProject(id, (p) => p.job.customer === "Novák");
    expect(p.job.address).toBe("Polní 5");
  });

  test("an empty job name is refused in the Job tab as in the title", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    const job = await openJobTab(app, "Valba");
    const name = job.getByLabel(cs("job.name"));
    await name.fill("");
    await name.press("Tab");
    await expect(name).toHaveValue("Valba");
    const title = page.locator(".editor input.title");
    await title.fill("  ");
    await title.press("Enter");
    await expect(title).toHaveValue("Valba");
    await page.waitForTimeout(700);
    expect((await loadProject(id)).name).toBe("Valba");
  });

  test("Back right after typing in a Job tab field stores the text", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    const job = await openJobTab(app, "Valba");
    await typeInto(page, job.getByLabel(cs("job.customer")), "Kočí");
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await waitForProject(id, (p) => p.job.customer === "Kočí");
  });

  test("Back right after typing in a number field stores the value", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    await selectWall(page, 0);
    await typeInto(page, bar(page, "def.title").getByLabel(cs("edge.pitch")), "23");
    await page.getByRole("button", { name: cs("editor.back") }).click();
    await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === 23);
  });
});
