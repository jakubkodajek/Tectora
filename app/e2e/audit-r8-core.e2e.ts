// Round-8 audit guards on the library side (audit 2026-11-22): the
// library's note names only what blocks work, says per entry what is
// blocked and goes once the entry is moved away (ARCH-701, UX-701,
// TEST-709); the bridge recalculates an older engine's stored result in a
// command of its own, once, and `load_project` only reads (HUNT-703,
// TEST-703).
import fs from "node:fs";
import path from "node:path";
import { LIBRARY_DIR } from "./env";
import { bridge, createExample, expect, loadProject, test } from "./fixtures";
import { cs } from "./i18n";

test.describe("round-8 library and open", () => {
  test("the library note names a file in place of the trash and a lock folder, not a stray file, and goes once fixed", async ({ app, page }) => {
    const id = await createExample("gable", "Sedlo Novák");
    fs.mkdirSync(path.join(LIBRARY_DIR, ".files"), { recursive: true });
    fs.writeFileSync(path.join(LIBRARY_DIR, ".files", "Thumbs.db"), "x");
    await app.open();
    await expect(page.getByText("Sedlo Novák").first()).toBeVisible();
    await expect(page.getByTestId("library-foreign")).toHaveCount(0);

    fs.rmSync(path.join(LIBRARY_DIR, ".trash"), { recursive: true, force: true });
    fs.writeFileSync(path.join(LIBRARY_DIR, ".trash"), "x");
    const lock = path.join(LIBRARY_DIR, ".activity", `${id}.lock`);
    fs.rmSync(lock, { force: true });
    fs.mkdirSync(path.join(lock, "inside"), { recursive: true });
    // Back to the window: the list is read again.
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    const note = page.getByTestId("library-foreign");
    await expect(note).toBeVisible();
    await expect(note).toContainText(cs("lib.foreign", { n: 2 }));
    const lines = note.getByTestId("library-foreign-line");
    await expect(lines).toHaveText([cs("lib.foreign.trash"), cs("lib.foreign.lock", { job: "Sedlo Novák" })]);
    await note.getByRole("button", { name: cs("lib.brokenShow") }).click();
    await expect(note.locator("li code")).toHaveText([".trash", `.activity/${id}.lock`]);
    await expect(note).not.toContainText("Thumbs.db");

    fs.rmSync(path.join(LIBRARY_DIR, ".trash"));
    fs.rmSync(lock, { recursive: true });
    await page.waitForTimeout(1100);
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect(note).toHaveCount(0);
  });

  test("the bridge recalculates an older engine's stored result once, and loading only reads", async ({ app: _app }) => {
    const id = await createExample("gable", "Nabídka");
    const p = await loadProject(id);
    p.result_snapshot = await bridge("take_snapshot", { project: p });
    p.job = { ...p.job, status: "offer" };
    await bridge("save_project", { id, project: p });
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const origin = path.join(LIBRARY_DIR, ".activity", `${id}.result.json`);
    expect(fs.existsSync(origin)).toBe(true);
    // As if engine 0.0.1 had stored figures 2 m² off.
    const job = JSON.parse(fs.readFileSync(file, "utf8"));
    const area = job.result_snapshot.roof_area as number;
    const engine = job.result_snapshot.engine_version as string;
    job.result_snapshot.engine_version = "0.0.1";
    job.result_snapshot.roof_area = area + 2;
    fs.writeFileSync(file, JSON.stringify(job));
    const origins = JSON.parse(fs.readFileSync(origin, "utf8")) as { checked: string }[];
    for (const o of origins) o.checked = "0.0.1";
    fs.writeFileSync(origin, JSON.stringify(origins));

    // Loading reads the job as it is.
    expect((await loadProject(id)).result_snapshot.engine_version).toBe("0.0.1");
    // The engine's change-log entry: its time and its summary (round 9, Design A item 7).
    const done = await bridge<{ entry_ts_ms: number; summary: string } | null>("refresh_stored_result", { id });
    const summary = done?.summary;
    expect(summary).toMatch(new RegExp(`^Stored result recalculated \\(Tectora 0\\.0\\.1 -> ${engine.replace(/\./g, "\\.")}\\)`));
    const now = await loadProject(id);
    expect(now.result_snapshot.engine_version).toBe(engine);
    expect(Math.abs(now.result_snapshot.roof_area - area)).toBeLessThan(0.01);
    const log = await bridge<{ ts_ms: number; actor: string; summary: string }[]>("project_activity", { id });
    expect(log[0]).toMatchObject({ actor: "tectora", summary, ts_ms: done?.entry_ts_ms });
    // Once only; an unknown job is no error.
    expect(await bridge("refresh_stored_result", { id })).toBeNull();
    expect(await bridge("refresh_stored_result", { id: "no-such-job" })).toBeNull();
  });
});
