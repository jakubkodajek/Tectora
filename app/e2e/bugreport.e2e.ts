// Report a bug (plan-delivery D14, ADR 0008): the colleague presses the
// button, types a few words and presses Send. The report goes to the relay
// (a Node `http` server in the test, `relay` in fixtures.ts, answering
// POST -> 302 -> GET like Apps Script); offline it waits in the outbox, a
// small indicator counts the waiting reports, and Send now delivers them.
// Waits are on the toast, the fixture's upload promise and the indicator; no
// sleeps. (The reports-folder tests of the first version went with the
// setting, ADR 0008.)
import fs from "node:fs";
import path from "node:path";
import { openEditor } from "./editor";
import { createExample, expect, loadProject, bridge, scratchFile, test, type App } from "./fixtures";
import type { Locator, Page } from "@playwright/test";
import { cs } from "./i18n";

const CUSTOMER = "Jan Novák-ZQX";
const ADDRESS = "Polní-ZQX 5, Zlín";

/** A job with a customer and an address, opened in the editor. */
async function jobWithCustomer(app: App, name: string) {
  const id = await createExample("hip", name);
  const p = await loadProject(id);
  p.job = { ...p.job, customer: CUSTOMER, address: ADDRESS };
  await bridge("save_project", { id, project: p });
  await app.open();
  await openEditor(app, name);
  return id;
}

const openDialog = async (page: Page) => {
  await page.getByRole("button", { name: cs("report.button") }).click();
  const dlg = page.getByRole("dialog", { name: cs("report.title") });
  await expect(dlg).toBeVisible();
  return dlg;
};

const sendButton = (dlg: Locator) => dlg.getByRole("button", { name: cs("report.send"), exact: true });

const indicator = (page: Page, count: number) => page.getByRole("button", { name: cs("report.outbox.count", { count }) });

const reportJson = (u: { zip: Map<string, Buffer> }) => JSON.parse(u.zip.get("report.json")!.toString("utf8"));

test.describe("report a bug", () => {
  test("Send files the report with the relay, without the customer in the issue data, and an AI client sees it as sent", async ({ app, page, mcp, relay }) => {
    await jobWithCustomer(app, "Valba chyba");
    const dlg = await openDialog(page);
    await expect(dlg).toContainText(cs("report.sendInfo"));
    await expect(dlg.getByRole("checkbox", { name: new RegExp(cs("report.inc.customer")) })).toBeChecked();
    await dlg.getByLabel(cs("report.what")).fill("Valba se po posunu stěny zlomila");
    const upload = relay.nextUpload();
    await sendButton(dlg).click();

    const u = await upload;
    await expect(page.getByRole("status").filter({ hasText: cs("report.sent") })).toBeVisible();
    await expect(dlg).toHaveCount(0);
    expect(u.body.file_name).toMatch(/^tectora-report-\d{8}-\d{6}-valba-chyba\.zip$/);
    expect(u.body.id).toMatch(/^[0-9a-f]{32}$/);
    expect(reportJson(u).description).toBe("Valba se po posunu stěny zlomila");
    expect(reportJson(u).source).toBe("app");
    // The issue data holds no customer name or address; the ZIP may.
    const meta = JSON.stringify(u.body.meta);
    expect(meta).not.toContain(CUSTOMER);
    expect(meta).not.toContain(ADDRESS);
    expect(u.body.meta.source).toBe("app");
    expect(u.body.meta.description).toBe("Valba se po posunu stěny zlomila");
    // No issue link is shown to the colleague.
    await expect(page.locator(".stack .toast").filter({ hasText: /github|issues\// })).toHaveCount(0);
    // Nothing waits.
    await expect(indicator(page, 1)).toHaveCount(0);

    const listed = await mcp.call("list_bug_reports", {});
    const mine = listed.reports.find((r: any) => r.file_name === u.body.file_name);
    expect(mine, JSON.stringify(listed)).toBeTruthy();
    expect(mine.state).toBe("sent");
  });

  test("offline the report waits, the indicator counts it, and Send now delivers it", async ({ app, page, relay }) => {
    await jobWithCustomer(app, "Bez site");
    await relay.refuse();
    const dlg = await openDialog(page);
    await dlg.getByLabel(cs("report.what")).fill("Napsáno bez připojení");
    await sendButton(dlg).click();

    await expect(page.getByRole("status").filter({ hasText: cs("report.queued") })).toBeVisible();
    await expect(dlg).toHaveCount(0);
    await expect(indicator(page, 1)).toBeVisible();
    // The draft was cleared: the report is safe in the outbox.
    const again = await openDialog(page);
    await expect(again.getByLabel(cs("report.what"))).toHaveValue("");
    await page.keyboard.press("Escape");
    await expect(again).toHaveCount(0);

    // The list names the waiting report and why.
    await indicator(page, 1).click();
    const list = page.getByRole("dialog", { name: cs("report.outbox.title") }).or(page.getByRole("region", { name: cs("report.outbox.title") }));
    await expect(list).toContainText(cs("report.state.waiting"));
    await expect(list).toContainText("offline");

    await relay.accept();
    const upload = relay.nextUpload();
    await list.getByRole("button", { name: cs("report.outbox.sendNow") }).click();
    const u = await upload;
    expect(reportJson(u).description).toBe("Napsáno bez připojení");
    await expect(page.getByRole("button", { name: cs("report.outbox.count", { count: 1 }) })).toHaveCount(0);
  });

  test("a waiting report can be discarded after asking, and the indicator goes", async ({ app, page, relay }) => {
    await jobWithCustomer(app, "Zahodit");
    await relay.refuse();
    const dlg = await openDialog(page);
    await dlg.getByLabel(cs("report.what")).fill("zahodit");
    await sendButton(dlg).click();
    await expect(indicator(page, 1)).toBeVisible();
    await indicator(page, 1).click();
    const list = page.getByRole("dialog", { name: cs("report.outbox.title") }).or(page.getByRole("region", { name: cs("report.outbox.title") }));
    // No: nothing happens.
    await app.answerConfirm(false);
    await list.getByRole("button", { name: cs("report.outbox.discard") }).click();
    await expect.poll(async () => (await app.dialogs()).filter((d) => d.message === cs("report.outbox.discardAsk")).length).toBe(1);
    await expect(indicator(page, 1)).toBeVisible();
    // Yes: the row and the indicator go.
    await app.answerConfirm(true);
    await list.getByRole("button", { name: cs("report.outbox.discard") }).click();
    await expect(indicator(page, 1)).toHaveCount(0);
  });

  test("while the report is sent the dialog shows it, the fields are not live and the text is kept until it is done", async ({ app, page, relay }) => {
    await jobWithCustomer(app, "Odesilani");
    const dlg = await openDialog(page);
    await dlg.getByLabel(cs("report.what")).fill("pomalé odeslání");
    relay.hold();
    const upload = relay.nextUpload();
    await sendButton(dlg).click();
    await expect(dlg.getByRole("button", { name: cs("report.sending") })).toBeVisible();
    await expect(dlg.getByLabel(cs("report.what"))).toBeDisabled();
    await expect(dlg.getByLabel(cs("report.what"))).toHaveValue("pomalé odeslání");
    relay.release();
    await upload;
    await expect(page.getByRole("status").filter({ hasText: cs("report.sent") })).toBeVisible();
    await expect(dlg).toHaveCount(0);
  });

  test("every checkbox decides what the report contains", async ({ app, page, relay }) => {
    const id = await jobWithCustomer(app, "Zaskrtavatka");
    const dlg = await openDialog(page);
    const box = (key: string) => dlg.getByRole("checkbox", { name: new RegExp(cs(key).replace(/[.*+?^${}()|[\]\\]/g, "\\$&").slice(0, 18)) });
    for (const key of ["report.inc.customer", "report.inc.history", "report.inc.snapshot", "report.inc.log"]) await box(key).uncheck();
    const upload = relay.nextUpload();
    await sendButton(dlg).click();
    const u = await upload;
    const r = reportJson(u);
    expect(r.job.id).toBe(id);
    expect(r.included).toMatchObject({ job: true, customer: false, history: false, snapshot: false, log: false });
    expect(r.job.anonymized).toBe(true);
    expect(r.actions).toEqual([]);
    expect(r.notices).toEqual([]);
    const names = [...u.zip.keys()];
    expect(names.filter((n) => n.startsWith("logs/") || n.startsWith("snapshot/") || n === "job/history.jsonl")).toEqual([]);
    expect(names).toContain("job/project.tectora");
    expect(u.zip.get("job/project.tectora")!.toString("utf8")).not.toContain(CUSTOMER);
    expect(JSON.stringify(u.body.meta)).not.toContain("Zaskrtavatka");
  });

  test("the optional name is sent when filled and remembered on this computer", async ({ app, page, relay }) => {
    await jobWithCustomer(app, "Jmeno");
    let dlg = await openDialog(page);
    await dlg.getByLabel(cs("report.yourName")).fill("Petr");
    let upload = relay.nextUpload();
    await sendButton(dlg).click();
    expect((await upload).body.meta.user_name).toBe("Petr");
    await expect(dlg).toHaveCount(0);
    dlg = await openDialog(page);
    await expect(dlg.getByLabel(cs("report.yourName"))).toHaveValue("Petr");
    await dlg.getByLabel(cs("report.yourName")).fill("");
    upload = relay.nextUpload();
    await sendButton(dlg).click();
    expect((await upload).body.meta.user_name ?? null).toBeNull();
  });

  test("closing the dialog keeps what was typed", async ({ app, page }) => {
    await createExample("hip", "Koncept");
    await app.open();
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

  test("Save as still writes a file and tells the user to pass it on", async ({ app, page }) => {
    await createExample("hip", "Ulozit");
    await app.open();
    await openEditor(app, "Ulozit");
    const own = path.join(path.dirname(scratchFile("x", "")), "Vlastní hlášení.zip");
    const dlg = await openDialog(page);
    await app.answerFileDialog(own);
    await dlg.getByRole("button", { name: cs("report.saveAs") }).click();
    await expect.poll(() => fs.existsSync(own)).toBe(true);
    const done = page.getByRole("status").filter({ hasText: cs("report.done") });
    await expect(done).toContainText(cs("report.passOn"));
    await expect(done.getByRole("button", { name: cs("report.show") })).toBeVisible();
    await expect(indicator(page, 1)).toHaveCount(0);
  });

  test("from the library: a hint to open the job first, no picture box, no folder anywhere", async ({ app, page }) => {
    await createExample("hip", "Knihovna");
    await app.open();
    const dlg = await openDialog(page);
    await expect(dlg).toContainText(cs("report.noJob"));
    await expect(dlg).toContainText(cs("report.libraryHint"));
    await expect(dlg).toContainText(cs("report.sendInfo"));
    await expect(dlg).not.toContainText(/složk/i);
    await expect(dlg.getByRole("checkbox", { name: new RegExp(cs("report.inc.snapshot").slice(0, 12)) })).toHaveCount(0);
    await expect(dlg.getByRole("checkbox", { name: new RegExp(cs("report.inc.log").slice(0, 12)) })).toBeChecked();
    await page.keyboard.press("Escape");
    await expect(dlg).toHaveCount(0);
  });

  test("Settings has no reports folder any more", async ({ app, page }) => {
    await app.open();
    await page.getByRole("button", { name: cs("settings.button"), exact: true }).click();
    const settings = page.getByRole("dialog", { name: cs("settings.title") });
    await expect(settings).toBeVisible();
    await expect(settings).not.toContainText(/hlášení chyb/i);
    await expect(settings.locator(".row").filter({ has: page.getByText(/reports/i) })).toHaveCount(0);
  });
});
