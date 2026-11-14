// Tab labels that do not all fit side by side are hidden (icons and their
// tooltips stay) instead of being cut: German "KI" was cut to "K" next to
// "Ergebn…" in a half-screen editor (review UI-005). Measured per language,
// so Czech keeps its labels where they fit.

/** Whether labels of `widths` px, each with `extra` px of icon, gap and padding, fit into `available` px. */
export function labelsFit(widths: number[], extra: number, available: number): boolean {
  const need = widths.reduce((s, w) => s + Math.ceil(w) + extra, 0);
  return need <= available + 0.5;
}

/**
 * `use:fitlabels` on a tab list: toggles the `compact` class when the
 * `.tab-label`s of its buttons do not fit at their full width.
 * `extra` is the room a button needs besides its label (icon, gap, padding).
 */
export function fitlabels(node: HTMLElement, extra = 26) {
  function check() {
    const labels = [...node.querySelectorAll<HTMLElement>(".tab-label")];
    if (!labels.length) return;
    const cs = getComputedStyle(node);
    const gap = parseFloat(cs.columnGap) || 0;
    const available = node.clientWidth - (parseFloat(cs.paddingLeft) || 0) - (parseFloat(cs.paddingRight) || 0) - gap * (labels.length - 1);
    // scrollWidth is the label's full text width also while it is cut or hidden.
    node.classList.toggle("compact", !labelsFit(labels.map((l) => l.scrollWidth), extra, available));
  }
  const resize = new ResizeObserver(check);
  resize.observe(node);
  // The UI language changes the labels.
  const texts = new MutationObserver(check);
  texts.observe(node, { subtree: true, characterData: true, childList: true });
  check();
  return {
    destroy() {
      resize.disconnect();
      texts.disconnect();
    },
  };
}
