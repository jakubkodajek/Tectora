<script lang="ts" module>
  export type MenuEntry =
    | { sep: true }
    | { label: string; icon?: string; hint?: string; danger?: boolean; disabled?: boolean; action: () => void };
  export interface MenuState {
    x: number;
    y: number;
    title?: string;
    entries: MenuEntry[];
  }
</script>

<script lang="ts">
  // Right-click menu of the plan and the 3D view: a small list at the
  // pointer, closed by a click outside, Escape, scrolling or losing focus.
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";

  let { menu = $bindable() }: { menu: MenuState | null } = $props();

  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ x: 0, y: 0 });

  $effect(() => {
    if (!menu) return;
    const m = menu;
    pos = { x: m.x, y: m.y };
    // Keep the menu inside the window.
    tick().then(() => {
      if (!el || menu !== m) return;
      const r = el.getBoundingClientRect();
      pos = {
        x: Math.max(4, Math.min(m.x, window.innerWidth - r.width - 4)),
        y: Math.max(4, Math.min(m.y, window.innerHeight - r.height - 4)),
      };
      el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    });
  });

  function close() {
    menu = null;
  }

  function run(e: Extract<MenuEntry, { action: () => void }>) {
    if (e.disabled) return;
    close();
    e.action();
  }

  function onkeydown(ev: KeyboardEvent) {
    if (!menu || !el) return;
    if (ev.key === "Escape") {
      ev.preventDefault();
      ev.stopPropagation();
      close();
      return;
    }
    if (ev.key !== "ArrowDown" && ev.key !== "ArrowUp") return;
    ev.preventDefault();
    const items = [...el.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = ev.key === "ArrowDown" ? (i + 1) % items.length : (i - 1 + items.length) % items.length;
    items[next]?.focus();
  }

  function onpointerdown(ev: PointerEvent) {
    if (menu && el && !el.contains(ev.target as Node)) close();
  }
</script>

<svelte:window onkeydowncapture={onkeydown} onpointerdowncapture={onpointerdown} onblur={close} onresize={close} onwheel={() => menu && close()} />

{#if menu}
  <div class="menu" role="menu" tabindex="-1" bind:this={el} style="left:{pos.x}px;top:{pos.y}px" oncontextmenu={(e) => e.preventDefault()}>
    {#if menu.title}<div class="title">{menu.title}</div>{/if}
    {#each menu.entries as e}
      {#if "sep" in e}
        <div class="sep" role="separator"></div>
      {:else}
        <button role="menuitem" class:danger={e.danger} disabled={e.disabled} onclick={() => run(e)}>
          <span class="ic">{#if e.icon}<Icon name={e.icon} size={15} />{/if}</span>
          <span class="label">{e.label}</span>
          {#if e.hint}<kbd>{e.hint}</kbd>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 50;
    min-width: 210px;
    max-width: 320px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow-lg);
    font-size: 13px;
  }
  .title {
    padding: 5px 9px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 9px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--surface-2);
    outline: none;
  }
  button:disabled {
    color: var(--text-3);
    cursor: default;
  }
  button.danger:not(:disabled) {
    color: var(--danger);
  }
  .ic {
    display: inline-flex;
    width: 15px;
    color: var(--text-2);
  }
  .danger .ic {
    color: var(--danger);
  }
  .label {
    flex: 1;
    white-space: nowrap;
  }
  kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    color: var(--text-3);
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
</style>
