// The calculation dialog asks the core for every figure (ARCH-004): what is
// typed is answered live, a value without solution says so and cannot be
// applied, and OK applies what the core worked out (the same as the MCP tool
// roof_helper).
import { bar, openEditor, selectWall, setField } from "./editor";
import { bridge, createExample, expect, loadProject, test, waitForProject } from "./fixtures";
import { cs } from "./i18n";

async function openCalc(page: import("@playwright/test").Page) {
  await selectWall(page, 0);
  await bar(page, "def.title").getByRole("button", { name: cs("calc.button") }).click();
  const dlg = page.getByRole("dialog", { name: cs("calc.title") });
  await expect(dlg).toBeVisible();
  return dlg;
}

test.describe("calculation helpers", () => {
  test("a simple triangle shows its pitch while typing and OK applies the core's value", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const dlg = await openCalc(page);
    const ok = dlg.getByRole("button", { name: cs("common.ok") });
    await expect(ok).toBeDisabled();
    await setField(dlg.getByRole("textbox", { name: cs("calc.width") }), "4");
    await setField(dlg.getByRole("textbox", { name: cs("calc.ridge") }), "7");
    await expect(dlg.locator(".result .row")).toContainText(cs("edge.pitch"));
    await expect(ok).toBeEnabled();
    const project = await loadProject(id);
    const want = await bridge<any>("roof_helper_preview", {
      project,
      part: 0,
      edge: 0,
      request: { tool: "triangle", width: 4, ridge: 7, width_includes_overhang: false },
    });
    await ok.click();
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg === want.patches["0"].pitch_deg);
    expect(p.parts[0].edges[0].pitch_deg).toBe(want.triangle.pitch_deg);
  });

  test("values without a solution say so, cannot be applied and give way to values that work", async ({ app, page }) => {
    await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const dlg = await openCalc(page);
    const ok = dlg.getByRole("button", { name: cs("common.ok") });
    // A rafter shorter than the width is no triangle.
    await setField(dlg.getByRole("textbox", { name: cs("calc.width") }), "6");
    await setField(dlg.getByRole("textbox", { name: cs("calc.rafter") }), "3");
    await expect(dlg.getByText(cs("calc.noResult"))).toBeVisible();
    await expect(ok).toBeDisabled();
    await setField(dlg.getByRole("textbox", { name: cs("calc.rafter") }), "7");
    await expect(dlg.getByText(cs("calc.noResult"))).toHaveCount(0);
    await expect(ok).toBeEnabled();
  });

  test("two facing walls: a typed ridge height gives both pitches, as the core works them out", async ({ app, page }) => {
    const id = await createExample("hip", "Valba");
    await openEditor(app, "Valba");
    const dlg = await openCalc(page);
    await dlg.getByRole("tab", { name: cs("calc.parallel") }).click();
    const ridge = dlg.getByRole("textbox", { name: cs("calc.ridge") });
    await expect(ridge).not.toHaveValue("");
    await setField(ridge, "7,5");
    const ok = dlg.getByRole("button", { name: cs("common.ok") });
    await expect(ok).toBeEnabled();
    const project = await loadProject(id);
    const want = await bridge<any>("roof_helper_preview", {
      project,
      part: 0,
      edge: 0,
      request: { tool: "parallel", target: "pitch", steps: [{ field: "ridge", value: 7.5 }], width_includes_overhang: true },
    });
    await ok.click();
    const p = await waitForProject(id, (p) => p.parts[0].edges[0].pitch_deg !== project.parts[0].edges[0].pitch_deg);
    for (const [k, patch] of Object.entries<any>(want.patches)) expect(p.parts[0].edges[Number(k)].pitch_deg).toBe(patch.pitch_deg);
  });
});
