// Every press in the plan goes through one guard (jobedits.ts rule 7).
// Round 6 fixed "a second finger replaces the running drag" (ERR-501) in
// four handlers; round 7 found the two without the check (UI-601: roof
// lines and wall lines). No DOM in these unit tests, so this reads the
// component: a new `onpointerdown` that does not go through `press` fails
// here. The behaviour itself (a second pointer on every kind of shape) is
// in app/e2e/audit-r7-drag.e2e.ts.
import { describe, expect, it } from "vitest";

const files = import.meta.glob("./PlanView.svelte", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const src = Object.values(files)[0];
const script = src.slice(0, src.indexOf("</script>"));
const markup = src.slice(src.indexOf("</script>"));

/** The body of `function name(` in the script, up to its closing brace at the same indent. */
function body(name: string): string {
  const at = script.indexOf(`  function ${name}(`);
  expect(at, name).toBeGreaterThan(0);
  return script.slice(at, script.indexOf("\n  }\n", at));
}

describe("PlanView: one press for every pointerdown", () => {
  it("every onpointerdown in the markup calls press(ev, …) and nothing else", () => {
    const handlers = [...markup.matchAll(/onpointerdown=\{([^\n]*)/g)].map((m) => m[1]);
    // The plan, buildings, walls, corners, dormers (area and front), chimneys, openings, windows, hand-drawn faces and lines.
    expect(handlers.length).toBeGreaterThanOrEqual(11);
    for (const h of handlers) expect(h).toMatch(/^\(ev\) => press\(ev, \{ on: "(plan|part|edge|vertex|item)"[^}]*\}?[^)]*\)\}/);
    // No shorthand `{onpointerdown}` and no listener added by hand.
    expect(markup).not.toMatch(/\{onpointerdown\}/);
    expect(script).not.toMatch(/addEventListener\("pointerdown"/);
  });

  it("press checks the other pointer before anything else", () => {
    const lines = body("press").split("\n").filter((l) => l.trim() && !l.trim().startsWith("//"));
    expect(lines[1].trim()).toBe("if (otherPointer(e) || locked) {");
  });

  it("only press hands a pointerdown on: the handlers per shape are called from press alone", () => {
    for (const name of ["pressPlan", "pressItem", "pressPart", "pressEdge", "pressVertex"]) {
      const calls = [...src.matchAll(new RegExp(`\\b${name}\\(`, "g"))].length;
      // Its definition and its one call in `press`.
      expect(calls, name).toBe(2);
      expect(body("press")).toContain(`${name}(e`);
    }
  });

  it("the pointer is captured in one place, and a drag starts only when the editor armed it", () => {
    expect(src.match(/setPointerCapture\(/g)?.length).toBe(1);
    expect(body("capture")).toContain("setPointerCapture(");
    const starts = [...script.matchAll(/\n    (objDrag|drag) = \{/g)];
    expect(starts.length).toBe(2);
    expect(body("startObjDrag")).toMatch(/if \(!arm\(e, target\)\) return;[\s\S]*objDrag = \{/);
    expect(body("pressVertex")).toMatch(/if \(!arm\(e, \{ type: "vertex", part, vertex \}\)\) return;[\s\S]*drag = \{/);
    // A live drag is never replaced.
    expect(body("arm")).toContain("if (objDrag || drag || armed) return false;");
  });

  it("every end of a press goes through endGesture, which tells the editor about each armed press", () => {
    expect(body("onpointerup")).toContain("endGesture(e)");
    expect(markup).toContain("onpointercancel={onpointerup}");
    expect(markup).toContain("onlostpointercapture={(e) => e.pointerId === gesture && endGesture(e)}");
    expect(script).toContain("onDestroy(() => endGesture());");
    const end = body("endGesture");
    // Cleared before any callback runs, and the end told in a finally.
    expect(end.indexOf("objDrag = null;")).toBeLessThan(end.indexOf("oncontext?.("));
    expect(end).toMatch(/finally \{\s*if \(a\) ondragend\?\.\(a, moved\);/);
    // A mouse that reports no button ends the press.
    expect(body("onpointermove")).toContain('if (gesture !== null && e.pointerType === "mouse" && e.buttons === 0) endGesture(e);');
  });
});
