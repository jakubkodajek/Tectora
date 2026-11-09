// A strip that scrolls sideways when it does not fit (editor toolbar, library
// filters): `use:hscroll` lets a plain mouse wheel scroll it, and the
// `hscroll` class in app.css keeps a thin scrollbar visible, so it is clear
// that more is there (macOS hides overlay scrollbars and a wheel only
// scrolls up and down).

export function hscroll(node: HTMLElement) {
  node.classList.add("hscroll");
  function onwheel(e: WheelEvent) {
    if (e.ctrlKey || Math.abs(e.deltaX) >= Math.abs(e.deltaY)) return;
    const max = node.scrollWidth - node.clientWidth;
    if (max <= 1) return;
    const o = getComputedStyle(node).overflowX;
    if (o !== "auto" && o !== "scroll") return;
    const next = Math.max(0, Math.min(max, node.scrollLeft + e.deltaY));
    if (next === node.scrollLeft) return;
    e.preventDefault();
    node.scrollLeft = next;
  }
  node.addEventListener("wheel", onwheel, { passive: false });
  return {
    destroy() {
      node.removeEventListener("wheel", onwheel);
    },
  };
}
