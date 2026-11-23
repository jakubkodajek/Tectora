// How many dialogs (Modal.svelte) are open now. The editor's prepare phase
// of Revert / the put-back waits while one is (round 13 design A); a count
// that leaks above 0 would refuse every later press until a restart
// (TEST-1301), so each opening gets its own release, which counts once.

let open = 0;

/** How many dialogs are open now. */
export function openModals(): number {
  return open;
}

/** Counts one dialog as open; the returned release counts it closed, once however often it is called. */
export function modalOpened(): () => void {
  open++;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    open--;
  };
}
