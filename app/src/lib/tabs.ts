// ARIA tabs pattern for the app's tab strips (dock, New job, Connect AI,
// calculation helpers): `use:tablist` on the `role="tablist"` element gives
// the selected tab the only Tab stop and lets the arrow keys, Home and End
// move to and select the next tab. Each strip names its panel with `tabIds`.

let counter = 0;

/** Ids for one tab strip: `tab(key)` for a tab, `panel` for its tabpanel. */
export function tabIds(prefix: string): { tab: (key: string) => string; panel: string } {
  const base = `${prefix}-${++counter}`;
  return { tab: (key) => `${base}-tab-${key}`, panel: `${base}-panel` };
}

const NEXT: Record<string, number> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };

export function tablist(node: HTMLElement) {
  const all = () => [...node.querySelectorAll<HTMLElement>('[role="tab"]')];
  const usable = () => all().filter((t) => !(t as HTMLButtonElement).disabled);

  function sync() {
    const tabs = all();
    const selected = tabs.find((t) => t.getAttribute("aria-selected") === "true") ?? usable()[0];
    for (const t of tabs) t.tabIndex = t === selected ? 0 : -1;
  }

  function onkeydown(e: KeyboardEvent) {
    const tabs = usable();
    const i = tabs.indexOf(document.activeElement as HTMLElement);
    if (i < 0 || e.altKey || e.ctrlKey || e.metaKey) return;
    let j: number;
    if (e.key in NEXT) j = (i + NEXT[e.key] + tabs.length) % tabs.length;
    else if (e.key === "Home") j = 0;
    else if (e.key === "End") j = tabs.length - 1;
    else return;
    // The editor's own arrow-key shortcuts must not act on the plan too.
    e.preventDefault();
    e.stopPropagation();
    tabs[j].focus();
    tabs[j].click();
  }

  const observer = new MutationObserver(sync);
  observer.observe(node, { subtree: true, childList: true, attributes: true, attributeFilter: ["aria-selected", "disabled"] });
  node.addEventListener("keydown", onkeydown);
  sync();
  return {
    destroy() {
      observer.disconnect();
      node.removeEventListener("keydown", onkeydown);
    },
  };
}
