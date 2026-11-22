// Guards for the round-9 audit (2026-11-22), input side.
//
// HUNT-802 / HUNT-804: a job whose numbers JavaScript changes on its round
// trip (a whole float like `1.0` in an unknown key, a stored result dated
// after 2^53 ms) saves from the editor; the impossible date is dropped with
// one warning when the job opens.
import fs from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { LIBRARY_DIR } from "./env";
import { createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

const jobTab = (page: Page) => page.locator("aside .tabs").getByRole("tab", { name: cs("dock.job") });

test.describe("round 9: numbers JavaScript changes (HUNT-802, HUNT-804)", () => {
  test("a job with 1.0 in an unknown key and a result dated after 2^53 ms saves from the editor", async ({ app, page }) => {
    const id = await createExample("gable", "Čísla");
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    p.result_snapshot = { engine_version: "0.9.0", taken_ms: 4242424242, roof_area: 99, ridge_height: 5, lines_by_code: { T: 20 } };
    // Written as another tool would: JSON.stringify can write neither.
    const text = JSON.stringify(p, null, 2)
      .replace("4242424242", "9007199254740993")
      .replace("{", '{\n  "vendor_scale": 1.0,\n  "vendor_o": {"a": [2.0, -0.0, 1e3]},');
    fs.writeFileSync(file, text);

    await app.open({ prefs: { followAi: false } });
    await app.openJob("Čísla");
    const warning = app.toasts().filter({ hasText: cs("err.result_dropped") });
    await expect(warning).toHaveCount(1);
    await expect(warning).toContainText("result_dropped");

    await jobTab(page).click();
    const customer = page.locator("aside .job").getByLabel(cs("job.customer"));
    await customer.click();
    await page.keyboard.type("Novák");
    await page.keyboard.press("Tab");
    const saved = await waitForProject(id, (q) => q.job?.customer === "Novák");
    await app.saved();
    expect(saved.result_snapshot ?? null).toBeNull();
    expect(saved.vendor_scale).toBe(1);
    expect(saved.vendor_o).toEqual({ a: [2, 0, 1000] });
    // No conflict was told, and the next open has nothing to warn about.
    await expect(app.toasts().filter({ hasText: "save_conflict" })).toHaveCount(0);
    expect((await loadProject(id)).job.customer).toBe("Novák");
  });
});
