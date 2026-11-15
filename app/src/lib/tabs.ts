// ARIA tabs and radio group patterns for the app's tab strips (dock, New
// job, Connect AI, calculation helpers) and the segmented choices built from
// buttons (eave / gable, camera views, theme, transform mode, …): `use:tablist`
// on the `role="tablist"` element, `use:radiogroup` on a `role="radiogroup"`
// element. The selected item gets the only Tab stop; the arrow keys, Home and
// End move to and select the next item. Each tab strip names its panel with
// `tabIds`.

let counter = 0;

/** Ids for one tab strip: `tab(key)` for a tab, `panel` for its tabpanel. */
export function tabIds(prefix: string): { tab: (key: string) => string; panel: string } {
  const base = `${prefix}-${++counter}`;
  return { tab: (key) => `${base}-tab-${key}`, panel: `${base}-panel` };
}

const NEXT: Record<string, number> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };

/**
 * Index of the item a key moves to among `count` usable items from `i`, or
 * null when the key does not move (wraps around at both ends).
 */
export function rovingTarget(key: string, i: number, count: number): number | null {
  if (count <= 0 || i < 0) return null;
  if (key in NEXT) return (i + NEXT[key] + count) % count;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  return null;
}

/** Roving focus over the `role` items in `node`, `attr` marking the selected one. */
function roving(node: HTMLElement, role: "tab" | "radio", attr: "aria-selected" | "aria-checked") {
  const all = () => [...node.querySelectorAll<HTMLElement>(`[role="${role}"]`)];
  const usable = () => all().filter((t) => !(t as HTMLButtonElement).disabled);

  function sync() {
    const items = all();
    const selected = items.find((t) => t.getAttribute(attr) === "true" && !(t as HTMLButtonElement).disabled) ?? usable()[0];
    for (const t of items) t.tabIndex = t === selected ? 0 : -1;
  }

  function onkeydown(e: KeyboardEvent) {
    const items = usable();
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (i < 0 || e.altKey || e.ctrlKey || e.metaKey) return;
    const j = rovingTarget(e.key, i, items.length);
    if (j === null) return;
    // The editor's own arrow-key shortcuts must not act on the plan too.
    e.preventDefault();
    e.stopPropagation();
    items[j].focus();
    items[j].click();
  }

  const observer = new MutationObserver(sync);
  observer.observe(node, { subtree: true, childList: true, attributes: true, attributeFilter: [attr, "disabled"] });
  node.addEventListener("keydown", onkeydown);
  sync();
  return {
    destroy() {
      observer.disconnect();
      node.removeEventListener("keydown", onkeydown);
    },
  };
}

export function tablist(node: HTMLElement) {
  return roving(node, "tab", "aria-selected");
}

/** Radio group of buttons (`role="radio"`, `aria-checked`): one Tab stop, arrows move and select (review UI-007). */
export function radiogroup(node: HTMLElement) {
  return roving(node, "radio", "aria-checked");
}
