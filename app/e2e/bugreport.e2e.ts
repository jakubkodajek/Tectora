// Report a bug (plan target/work/bug-report, criteria 14, 15 and 17): set the
// reports folder in Settings, open a job, press the top-bar button, type a
// text and Send; the MCP server then lists and reads that report.
// Written before the feature exists. Waits on the toast, never on a sleep.
import fs from "node:fs";
import path from "node:path";
import { openEditor } from "./editor";
import { createExample, expect, scratchFile, test } from "./fixtures";
import { cs } from "./i18n";

test.describe("report a bug", () => {
  test("the reports folder from Settings gets the report, which an AI client can list and read", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba chyba");
    const folder = path.join(path.dirname(scratchFile("x", "")), "Hlášení");
    fs.mkdirSync(folder, { recursive: true });

    await page.getByRole("button", { name: cs("settings.button"), exact: true }).click();
    const settings = page.getByRole("dialog", { name: cs("settings.title") });
    const row = settings.locator(".row").filter({ has: page.getByText(cs("settings.reportsDir"), { exact: true }) });
    await app.answerFileDialog(folder);
    await row.getByRole("button", { name: cs("settings.change"), exact: true }).click();
    await expect(row.locator("code.path")).toHaveText(folder);
    await page.keyboard.press("Escape");
    await expect(settings).toHaveCount(0);

    await openEditor(app, "Valba chyba");
    const button = page.getByRole("button", { name: cs("report.button") });
    await expect(button).toBeVisible();
    await button.click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    await expect(dlg.getByLabel(cs("report.what"))).toBeVisible();
    await expect(dlg.getByRole("checkbox", { name: new RegExp(cs("report.inc.customer")) })).toBeChecked();
    await expect(dlg.getByRole("checkbox", { name: new RegExp(cs("report.inc.log").slice(0, 20)) })).toBeChecked();
    await dlg.getByLabel(cs("report.what")).fill("Valba se po posunu stěny zlomila");
    await dlg.getByRole("button", { name: cs("report.send"), exact: true }).click();

    const done = page.getByRole("status").filter({ hasText: cs("report.done") });
    await expect(done).toBeVisible();
    await expect(done.getByRole("button", { name: cs("report.show") })).toBeVisible();
    await expect(dlg).toHaveCount(0);

    const files = fs.readdirSync(folder).filter((n) => n.endsWith(".zip"));
    expect(files).toHaveLength(1);
    expect(files[0]).toMatch(/^tectora-report-\d{8}-\d{6}-valba-chyba\.zip$/);
    expect(fs.readdirSync(folder).filter((n) => n.endsWith(".part"))).toEqual([]);

    const listed = await mcp.call("list_bug_reports", {});
    expect(JSON.stringify(listed)).toContain(files[0]);
    const read = await mcp.call("read_bug_report", { name: files[0] });
    expect(read.report.description).toBe("Valba se po posunu stěny zlomila");
    expect(read.report.job.id).toBe(id);
    expect(read.report.source).toBe("app");
    expect(read.report.ui.locale).toBe("cs");
  });

  test("the button is in the library too, and Send works with an empty text", async ({ app, page }) => {
    await createExample("hip", "Bez textu");
    const button = page.getByRole("button", { name: cs("report.button") });
    await expect(button).toBeVisible();
    await button.click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    await expect(dlg).toContainText(cs("report.noJob"));
    await expect(dlg.getByRole("button", { name: cs("report.send"), exact: true }).or(dlg.getByRole("button", { name: cs("report.saveAs") }))).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dlg).toHaveCount(0);
  });
});
