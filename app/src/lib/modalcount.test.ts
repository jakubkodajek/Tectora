// TEST-1301: a dialog that opens and closes leaves the dialog count at 0, so
// a later Revert / put-back is not held by a dialog that is gone.
import { describe, expect, it } from "vitest";
import { modalOpened, openModals } from "./modalcount";
import modalSource from "./Modal.svelte?raw";

describe("the dialog count (TEST-1301)", () => {
  it("is back to 0 after dialogs open and close, in any order", () => {
    const base = openModals();
    const a = modalOpened();
    const b = modalOpened();
    expect(openModals()).toBe(base + 2);
    a();
    expect(openModals()).toBe(base + 1);
    b();
    expect(openModals()).toBe(base);
  });

  it("counts a release once however often it is called", () => {
    const base = openModals();
    const a = modalOpened();
    a();
    a();
    expect(openModals()).toBe(base);
    const b = modalOpened();
    expect(openModals()).toBe(base + 1);
    b();
    expect(openModals()).toBe(base);
  });

  it("Modal counts itself open on mount and releases in its teardown", () => {
    const src = modalSource.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`\\])\/\/[^\n]*/g, "$1");
    const mount = src.slice(src.indexOf("onMount(() => {"));
    expect(mount, "the first thing onMount does").toMatch(/^onMount\(\(\) => \{\s*const (\w+) = modalOpened\(\);/);
    const name = mount.match(/const (\w+) = modalOpened\(\);/)![1];
    expect(mount, "the teardown releases first").toMatch(new RegExp(`return \\(\\) => \\{\\s*${name}\\(\\);`));
    expect(src.match(/modalOpened\(/g), "one opening per dialog").toHaveLength(1);
  });
});
