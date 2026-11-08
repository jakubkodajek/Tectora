// Settings: language, theme, roof defaults (used by new roofs), AI follow
// mode, folders; all written to the settings file the MCP server shares, and
// MCP changes to the settings reach the app.
import fs from "node:fs";
import path from "node:path";
import { CONFIG_DIR, E2E_DIR } from "./env";
import { clickPlan, openEditor, plan } from "./editor";
import { bridge, createExample, expect, listProjects, test, waitForProject } from "./fixtures";
import { cs, tr } from "./i18n";

const settingsFile = () => JSON.parse(fs.readFileSync(path.join(CONFIG_DIR, "settings.json"), "utf8"));

async function openSettings(app: import("./fixtures").App) {
  await app.page.getByRole("button", { name: cs("settings.button") }).click();
  const dlg = app.page.getByRole("dialog", { name: cs("settings.title") });
  await expect(dlg).toBeVisible();
  return dlg;
}

test.describe("settings", () => {
  test("language from the settings and from the header picker, stored in the settings file", async ({ app, page, mcp }) => {
    await app.open();
    const dlg = await openSettings(app);
    await dlg.getByRole("combobox", { name: cs("lang") }).selectOption("en");
    await expect(page.getByRole("dialog", { name: tr("en", "settings.title") })).toBeVisible();
    await expect.poll(() => settingsFile().locale).toBe("en");
    expect((await mcp.call("get_settings")).settings.locale).toBe("en");
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: tr("en", "lib.new") }).first()).toBeVisible();

    await page.locator("header label.lang select").selectOption("de");
    await expect(page.getByRole("button", { name: tr("de", "lib.new") }).first()).toBeVisible();
    await expect.poll(() => settingsFile().locale).toBe("de");
    await page.reload();
    await expect(page.getByRole("button", { name: tr("de", "lib.new") }).first()).toBeVisible();
    await expect(page.locator("html")).toHaveAttribute("lang", /de|cs/);
  });

  test("theme buttons switch light, dark and system", async ({ app, page }) => {
    await app.open();
    const dlg = await openSettings(app);
    const html = page.locator("html");
    await dlg.getByRole("radio", { name: cs("settings.theme.dark") }).click();
    await expect(html).toHaveAttribute("data-theme", "dark");
    await expect.poll(() => settingsFile().theme).toBe("dark");
    const darkBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    await dlg.getByRole("radio", { name: cs("settings.theme.light") }).click();
    await expect(html).toHaveAttribute("data-theme", "light");
    const lightBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(lightBg).not.toBe(darkBg);
    await dlg.getByRole("radio", { name: cs("settings.theme.system") }).click();
    await expect(html).not.toHaveAttribute("data-theme", /./);
    await expect.poll(() => settingsFile().theme).toBe("system");
  });

  test("roof defaults: valid values are used by new roofs, invalid ones are refused", async ({ app, page }) => {
    await app.open();
    const dlg = await openSettings(app);
    const grid = dlg.locator(".grid3");
    await grid.getByLabel(cs("edge.pitch")).fill("27.5");
    await grid.getByLabel(cs("edge.pitch")).press("Tab");
    await grid.getByLabel(cs("edge.eaveHeight")).fill("2.6");
    await grid.getByLabel(cs("edge.eaveHeight")).press("Tab");
    await grid.getByLabel(cs("edge.overhang")).fill("0.4");
    await grid.getByLabel(cs("edge.overhang")).press("Tab");
    const saved = { pitch_deg: 27.5, eave_height: 2.6, overhang: 0.4 };
    await expect.poll(() => settingsFile().roof_defaults).toEqual(saved);
    // The file is written before the app has the answers to the three saves;
    // each answer sets the fields again, which would overwrite a value typed
    // in the meantime. The app keeps the last answer in localStorage.
    await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("tectora.presets") ?? "null"))).toEqual(saved);
    await grid.getByLabel(cs("edge.pitch")).fill("95");
    await grid.getByLabel(cs("edge.pitch")).press("Tab");
    await expect(dlg.getByRole("alert")).toHaveText(cs("settings.err.invalid_value"));
    expect(settingsFile().roof_defaults.pitch_deg).toBe(27.5);
    await page.keyboard.press("Escape");

    // A drawn roof uses the defaults.
    await page.locator("header").getByRole("button", { name: cs("lib.new") }).click();
    const nj = page.getByRole("dialog", { name: cs("new.title") });
    await nj.getByRole("tab", { name: cs("new.draw") }).click();
    await nj.getByRole("button", { name: cs("new.create") }).click();
    const id = (await listProjects())[0].id;
    await clickPlan(page, 0, 0);
    const len = page.getByRole("toolbar", { name: cs("draw.title") }).getByLabel(cs("draw.length"));
    await len.fill("5");
    await len.press("ArrowRight");
    await len.press("ArrowUp");
    await len.press("ArrowLeft");
    await len.press("Enter");
    const p = await waitForProject(id, (p) => p.parts.length === 1);
    expect(p.parts[0].edges[0]).toMatchObject({ pitch_deg: 27.5, eave_height: 2.6, overhang: 0.4 });
  });

  test("follow AI toggle is stored; version and settings file are shown", async ({ app, page }) => {
    await app.open();
    const dlg = await openSettings(app);
    const follow = dlg.getByRole("checkbox", { name: new RegExp(cs("settings.followAi")) });
    await expect(follow).toBeChecked();
    await follow.uncheck();
    await expect.poll(() => settingsFile().follow_ai).toBe(false);
    await follow.check();
    await expect.poll(() => settingsFile().follow_ai).toBe(true);
    await expect(dlg.locator(".about")).toContainText(path.join(CONFIG_DIR, "settings.json"));
    await dlg.getByRole("button", { name: cs("common.close") }).click();
    await expect(dlg).toHaveCount(0);
  });

  test("drawings folder: change, reset; a relative path is refused with a message", async ({ app, page }) => {
    await app.open();
    const dlg = await openSettings(app);
    const target = path.join(E2E_DIR, "files", "Výkresy TERRAN");
    fs.mkdirSync(target, { recursive: true });
    const row = dlg.locator(".row").filter({ hasText: cs("settings.drawingsDir") });
    await app.answerFileDialog(target);
    await row.getByRole("button", { name: cs("settings.change") }).click();
    await expect(row.locator("code.path")).toHaveText(target);
    await expect.poll(() => settingsFile().drawings_dir).toBe(target);
    await row.getByRole("button", { name: cs("settings.reset") }).click();
    await expect(row.getByRole("button", { name: cs("settings.reset") })).toHaveCount(0);
    await expect.poll(() => settingsFile().drawings_dir ?? null).toBeNull();

    await app.answerFileDialog("relative/folder");
    await row.getByRole("button", { name: cs("settings.change") }).click();
    await expect(row.getByRole("alert")).toHaveText(cs("settings.err.not_absolute"));
  });

  test("settings changed over MCP reach the open app", async ({ app, page, mcp }) => {
    await app.open();
    await mcp.call("update_settings", { roof_defaults: { pitch_deg: 33 }, theme: "dark", locale: "sk", summary: "Defaults as the user asked" });
    // No reload: the app follows the settings file.
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await expect(page.getByRole("button", { name: tr("sk", "lib.new") }).first()).toBeVisible();
    await page.getByRole("button", { name: tr("sk", "settings.button") }).click();
    const dlg = page.getByRole("dialog", { name: tr("sk", "settings.title") });
    await expect(dlg.locator(".grid3").getByLabel(tr("sk", "edge.pitch"))).toHaveValue("33");
  });

  test("the jobs folder changed while a job is open closes the job (ERR-019)", async ({ app, page }) => {
    await createExample("gable", "Sedlo");
    await openEditor(app, "Sedlo");
    // As an AI client changing the folder over MCP: the settings file changes.
    const other = path.join(E2E_DIR, "other-library");
    fs.rmSync(other, { recursive: true, force: true });
    fs.mkdirSync(other, { recursive: true });
    await bridge("update_settings", { patch: { projects_dir: other } });
    // The open job belonged to the old folder: back to the library.
    await expect(plan(page)).toHaveCount(0);
    await expect(page.getByRole("button", { name: cs("settings.button") })).toBeVisible();
  });
});
