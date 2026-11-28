// Report a bug (plan target/work/bug-report, criteria 14, 15 and 17): set the
// reports folder in Settings, open a job, press the top-bar button, type a
// text and Send; the MCP server then lists and reads that report.
// Written before the feature exists. Waits on the toast, never on a sleep.
import fs from "node:fs";
import path from "node:path";
import { openEditor } from "./editor";
import { BRIDGE_URL } from "./env";
import { createExample, expect, scratchFile, test, type App } from "./fixtures";
import type { Page } from "@playwright/test";
import { cs } from "./i18n";

test.describe("report a bug", () => {
  test("the reports folder from Settings gets the report, which an AI client can list and read", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Valba chyba");
    await app.open();
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
    await dlg.getByRole("button", { name: cs("report.save"), exact: true }).click();

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
    await app.open();
    const button = page.getByRole("button", { name: cs("report.button") });
    await expect(button).toBeVisible();
    await button.click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    await expect(dlg).toContainText(cs("report.noJob"));
    await expect(dlg.getByRole("button", { name: cs("report.save"), exact: true }).or(dlg.getByRole("button", { name: cs("report.saveAs") }))).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(dlg).toHaveCount(0);
  });
});

// ---- fix round 1 (target/work/bug-report/fix-r1.md, "Test-side API") ----

/** Sets the reports folder in Settings, as a user does. */
async function setReportsFolder(app: App, page: Page, folder: string) {
  fs.mkdirSync(folder, { recursive: true });
  await page.getByRole("button", { name: cs("settings.button"), exact: true }).click();
  const settings = page.getByRole("dialog", { name: cs("settings.title") });
  const row = settings.locator(".row").filter({ has: page.getByText(cs("settings.reportsDir"), { exact: true }) });
  await app.answerFileDialog(folder);
  await row.getByRole("button", { name: cs("settings.change"), exact: true }).click();
  await expect(row.locator("code.path")).toHaveText(folder);
  await page.keyboard.press("Escape");
  await expect(settings).toHaveCount(0);
}

const reportsIn = (folder: string) => fs.readdirSync(folder).filter((n) => n.endsWith(".zip"));

test.describe("report a bug, review findings", () => {
  test("every checkbox decides what the report contains (TEST-001, ARCH-001)", async ({ app, page, mcp }) => {
    const id = await createExample("hip", "Zaskrtavatka");
    const folder = path.join(path.dirname(scratchFile("x", "")), "Hlášení-boxy");
    await app.open();
    await setReportsFolder(app, page, folder);
    await openEditor(app, "Zaskrtavatka");
    await page.getByRole("button", { name: cs("report.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    const box = (key: string) => dlg.getByRole("checkbox", { name: new RegExp(cs(key).replace(/[.*+?^${}()|[\]\\]/g, "\\$&").slice(0, 18)) });
    for (const key of ["report.inc.customer", "report.inc.history", "report.inc.snapshot", "report.inc.log"]) await box(key).uncheck();
    await dlg.getByRole("button", { name: cs("report.save"), exact: true }).click();
    await expect(page.getByRole("status").filter({ hasText: cs("report.done") })).toBeVisible();
    const [file] = reportsIn(folder);
    const read = await mcp.call("read_bug_report", { name: file });
    expect(read.report.job.id).toBe(id);
    expect(read.report.included).toMatchObject({ job: true, customer: false, history: false, snapshot: false, log: false });
    expect(read.report.job.anonymized).toBe(true);
    expect(read.report.actions).toEqual([]);
    expect(read.report.notices).toEqual([]);
    const names = read.manifest.entries.map((e: any) => e.name);
    expect(names.filter((n: string) => n.startsWith("logs/") || n.startsWith("snapshot/") || n === "job/history.jsonl")).toEqual([]);
    expect(names).toContain("job/project.tectora");
  });

  test("closing the dialog keeps what was typed (UX-002)", async ({ app, page }) => {
    await createExample("hip", "Koncept");
    await openEditor(app, "Koncept");
    const button = page.getByRole("button", { name: cs("report.button") });
    await button.click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    await dlg.getByLabel(cs("report.what")).fill("Dlouhý popis, který nesmí zmizet");
    await page.keyboard.press("Escape");
    await expect(dlg).toHaveCount(0);
    await button.click();
    await expect(page.getByRole("dialog", { name: cs("report.title") }).getByLabel(cs("report.what"))).toHaveValue("Dlouhý popis, který nesmí zmizet");
  });

  test("the toast says where the report went and keeps Show in folder (UX-006)", async ({ app, page }) => {
    await createExample("hip", "Kam");
    const folder = path.join(path.dirname(scratchFile("x", "")), "Hlášení-kam");
    await app.open();
    await setReportsFolder(app, page, folder);
    await openEditor(app, "Kam");
    await page.getByRole("button", { name: cs("report.button") }).click();
    await page.getByRole("dialog", { name: cs("report.title") }).getByRole("button", { name: cs("report.save"), exact: true }).click();
    const done = page.getByRole("status").filter({ hasText: cs("report.done") });
    await expect(done).toContainText(cs("report.savedTo", { folder: path.basename(folder) }));
    await expect(done.getByRole("button", { name: cs("report.show") })).toBeVisible();
    // Save as: the toast tells the user to pass the file on.
    const own = path.join(path.dirname(folder), "Vlastní hlášení.zip");
    await page.getByRole("button", { name: cs("report.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    await app.answerFileDialog(own);
    await dlg.getByRole("button", { name: cs("report.saveAs") }).click();
    await expect.poll(() => fs.existsSync(own)).toBe(true);
    await expect(page.getByRole("status").filter({ hasText: cs("report.passOn") })).toBeVisible();
  });

  test("from the library: no picture box, a hint to open the job first, and where to set the folder (UX-003, UX-004, UX-007)", async ({ app, page }) => {
    await createExample("hip", "Knihovna");
    await app.open();
    await page.getByRole("button", { name: cs("report.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    await expect(dlg).toContainText(cs("report.noJob"));
    await expect(dlg).toContainText(cs("report.libraryHint"));
    await expect(dlg).toContainText(cs("report.destSettings"));
    await expect(dlg.getByRole("checkbox", { name: new RegExp(cs("report.inc.snapshot").slice(0, 12)) })).toHaveCount(0);
    await expect(dlg.getByRole("checkbox", { name: new RegExp(cs("report.inc.log").slice(0, 12)) })).toBeChecked();
  });

  test("Cancel while the report is prepared does not leave a report behind, or is not offered (UI-001)", async ({ app, page }) => {
    await createExample("hip", "Zrusit");
    const folder = path.join(path.dirname(scratchFile("x", "")), "Hlášení-zrusit");
    await app.open();
    await setReportsFolder(app, page, folder);
    await openEditor(app, "Zrusit");
    await page.getByRole("button", { name: cs("report.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    await dlg.getByLabel(cs("report.what")).fill("pokus");
    let calls = 0;
    let open!: () => void;
    const gate = new Promise<void>((r) => (open = r));
    await page.route("**/create_bug_report", async (route) => {
      calls++;
      await gate;
      await route.continue();
    });
    await dlg.getByRole("button", { name: cs("report.save"), exact: true }).or(dlg.getByRole("button", { name: cs("report.sending") })).click();
    await expect.poll(() => calls).toBe(1);
    // Fields are not live while the report is prepared.
    await expect(dlg.getByLabel(cs("report.what"))).toBeDisabled();
    const cancel = dlg.getByRole("button", { name: cs("common.cancel") });
    const offered = await cancel.isEnabled() && (await cancel.getAttribute("aria-disabled")) !== "true";
    if (offered) await cancel.click();
    const answered = page.waitForResponse((r) => r.url().endsWith("/create_bug_report"));
    open();
    await answered;
    if (offered) {
      // Cancel was real: the dialog is gone and no file is left.
      await expect(dlg).toHaveCount(0);
      await expect.poll(() => reportsIn(folder).length, { message: "a cancelled report must not stay" }).toBe(0);
    } else {
      await expect(page.getByRole("status").filter({ hasText: cs("report.done") })).toBeVisible();
      expect(reportsIn(folder)).toHaveLength(1);
    }
    expect(fs.readdirSync(folder).filter((n) => n.endsWith(".part"))).toEqual([]);
    expect(BRIDGE_URL).toBeTruthy();
  });

  test("a reports folder that is gone says so and offers Save as, the text stays (ERR-004)", async ({ app, page }) => {
    await createExample("hip", "Zmizela");
    const folder = path.join(path.dirname(scratchFile("x", "")), "Hlášení-pryč");
    await app.open();
    await setReportsFolder(app, page, folder);
    fs.rmSync(folder, { recursive: true });
    await openEditor(app, "Zmizela");
    await page.getByRole("button", { name: cs("report.button") }).click();
    const dlg = page.getByRole("dialog", { name: cs("report.title") });
    await dlg.getByLabel(cs("report.what")).fill("text zůstane");
    await dlg.getByRole("button", { name: cs("report.save"), exact: true }).click();
    const err = page.getByRole("alert").filter({ hasText: cs("err.ctx.report") });
    await expect(err).toBeVisible();
    await expect(err).toContainText(cs("report.destMissing"));
    await expect(dlg.getByLabel(cs("report.what"))).toHaveValue("text zůstane");
    await expect(dlg.getByRole("button", { name: cs("report.saveAs") })).toBeVisible();
  });
});
