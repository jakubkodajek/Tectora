// A press on a button that is gone once its action is done (the history's
// Revert and put-back, the result card's put-back and Store): from the
// keyboard (the button has the focus ring) the focus would drop to the
// page's body, and the keyboard user starts again from the window's top
// (audit 2026-11-22 UI-902, 2026-11-23 UI-1001). Here the focus lands on
// `fallback` (the history list, the card's heading) instead.

import { tick } from "svelte";

/** How long to wait (ms) for a button its action removes only once a later answer is in (Store: the card changes with the next calculation). */
const WAIT_MS = 5000;

/** Runs `run`; pressed from the keyboard, the focus ends on `fallback()` when the button went away. */
export async function pressKeepingFocus(run: () => void | Promise<void>, fallback: () => HTMLElement | null | undefined): Promise<void> {
  const pressed = document.activeElement;
  const keyboard = !!pressed?.matches(":focus-visible");
  await run();
  if (!keyboard) return;
  await tick();
  const end = Date.now() + WAIT_MS;
  while (Date.now() < end) {
    const a = document.activeElement;
    if (!a || a === document.body || !a.isConnected) {
      // The card itself may be drawn again only with the next answer (Store: the new stored result's card).
      const to = fallback();
      if (to?.isConnected) {
        to.focus();
        return;
      }
    }
    // The focus moved on to something else the user chose: leave it.
    else if (a !== pressed) return;
    await new Promise((r) => setTimeout(r, 50));
  }
}
