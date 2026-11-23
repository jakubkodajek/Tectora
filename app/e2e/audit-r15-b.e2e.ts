// Guards for the round-15 audit (2026-11-23), part B (notices, limits, wording).
//
// ERR-1401  an open job another program made unreadable: the notice says what
//           the next save does, "replaced" after it; leaving without an edit
//           writes nothing
import fs from "node:fs";
import path from "node:path";
import { bar, openEditor, selectWall, setField } from "./editor";
import { LIBRARY_DIR } from "./env";
import { createExample, expect, noToast, test } from "./fixtures";
import { cs } from "./i18n";

// TECTORA_SHOTS=<folder>/ saves a screenshot after each test (fix evidence).
test.afterEach(async ({ page }, info) => {
  const dir = process.env.TECTORA_SHOTS;
  if (!dir) return;
  const id = info.title.match(/(HUNT|UX|UI|ERR|TEST|ARCH)-\d+/)?.[0] ?? "r15";
  const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
  await page.screenshot({ path: `${dir}r15-${id}-${n}-${process.env.TECTORA_SHOTS_TAG ?? "after"}.png`, timeout: 10_000 }).catch(() => {});
});

/** `{"a": {"a": … {}}}` with `n` objects inside each other. */
function nestedObjects(n: number): unknown {
  let v: unknown = {};
  for (let k = 1; k < n; k++) v = { a: v };
  return v;
}

/** The job file with 40 levels of unknown keys another program wrote. */
function writeTooDeep(file: string): string {
  const outside = JSON.parse(fs.readFileSync(file, "utf8"));
  outside.crm = nestedObjects(40);
  const text = JSON.stringify(outside);
  fs.writeFileSync(file, text);
  return text;
}

test.describe("round 15, part B", () => {
  test("an open job made too deep to read from outside says the next save replaces it, and says so after it (ERR-1401)", async ({ app, page }) => {
    const id = await createExample("hip", "R15 cizí");
    await openEditor(app, "R15 cizí");
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    writeTooDeep(file);
    const notice = app.toasts().filter({ hasText: cs("editor.outsideUnreadable") });
    await expect(notice).toHaveCount(1);
    await expect(notice).toContainText("R15 cizí");
    // Not the open-time advice: remove the data in that program, or a backup.
    await expect(app.toasts().filter({ hasText: cs("err.unknown_keys") })).toHaveCount(0);
    await expect(app.toasts().filter({ hasText: cs("err.unreadable") })).toHaveCount(0);
    await selectWall(page, 0);
    await setField(bar(page, "def.title").getByLabel(cs("edge.pitch")), 31);
    await app.saved();
    await expect(app.toasts().filter({ hasText: cs("editor.outsideReplaced") })).toHaveCount(1);
    const p = JSON.parse(fs.readFileSync(file, "utf8"));
    expect([p.parts[0].edges[0].pitch_deg, "crm" in p]).toEqual([31, false]);
  });

  test("leaving an open job made unreadable from outside without an edit keeps that program's file (ERR-1401)", async ({ app, page }) => {
    const id = await createExample("hip", "R15 cizí zpět");
    await openEditor(app, "R15 cizí zpět");
    await app.saved();
    const file = path.join(LIBRARY_DIR, `${id}.tectora`);
    const text = writeTooDeep(file);
    await expect(app.toasts().filter({ hasText: cs("editor.outsideUnreadable") })).toHaveCount(1);
    await page.getByRole("button", { name: cs("editor.back") }).first().click();
    await expect(page.locator("header .brand")).toBeVisible();
    await noToast(app.toasts().filter({ hasText: cs("editor.outsideReplaced") }));
    expect(fs.readFileSync(file, "utf8")).toBe(text);
  });
});
