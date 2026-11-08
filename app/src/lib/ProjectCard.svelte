<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import PlanThumb from "./PlanThumb.svelte";
  import { m2, relativeTime } from "./format";
  import { t } from "./i18n.svelte";
  import type { ProjectSummary } from "./types";

  let {
    project,
    onopen,
    onduplicate,
    onreuse,
    ondelete,
  }: {
    project: ProjectSummary;
    onopen: () => void;
    onduplicate: () => void;
    /** Start a new job (another customer) from this roof. */
    onreuse: () => void;
    ondelete: () => void;
  } = $props();

  let menuOpen = $state(false);
  let moreBtn: HTMLButtonElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();

  // Opening moves the focus to the first item; the arrow keys, Home and End
  // move between the items; Escape or Tab closes it (Escape returns the
  // focus to the "More actions" button), like the editor's context menu.
  $effect(() => {
    if (menuOpen) tick().then(() => menuEl?.querySelector<HTMLButtonElement>("button")?.focus());
  });

  function closeMenu(refocus: boolean) {
    menuOpen = false;
    if (refocus) moreBtn?.focus();
  }

  function onmenukey(e: KeyboardEvent) {
    if (!menuEl) return;
    const items = [...menuEl.querySelectorAll<HTMLButtonElement>("button")];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    let j: number | null = null;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeMenu(true);
      return;
    }
    if (e.key === "Tab") {
      closeMenu(false);
      return;
    }
    if (e.key === "ArrowDown") j = (i + 1) % items.length;
    else if (e.key === "ArrowUp") j = (i - 1 + items.length) % items.length;
    else if (e.key === "Home") j = 0;
    else if (e.key === "End") j = items.length - 1;
    if (j === null) return;
    e.preventDefault();
    items[j].focus();
  }
</script>

<svelte:window onclick={() => (menuOpen = false)} />

<article class="card" ondblclick={onopen}>
  <button class="thumb" onclick={onopen} aria-label="{t('card.open')} {project.name}">
    <PlanThumb outlines={project.outlines} lines={project.lines} />
  </button>
  <div class="meta">
    <div class="text">
      <h3 title={project.name}>{project.name}</h3>
      <p class="muted">{relativeTime(project.modified_ms)} · {m2(project.roof_area)}</p>
    </div>
    <div class="menu-wrap">
      <button
        class="icon-btn"
        bind:this={moreBtn}
        aria-label={t("card.more")}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onclick={(e) => {
          e.stopPropagation();
          menuOpen = !menuOpen;
        }}
      >
        <Icon name="more" />
      </button>
      {#if menuOpen}
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <div class="menu" role="menu" tabindex="-1" bind:this={menuEl} onkeydown={onmenukey}>
          <button role="menuitem" onclick={onopen}><Icon name="open" size={16} />{t("card.open")}</button>
          <button role="menuitem" onclick={onduplicate}><Icon name="copy" size={16} />{t("card.duplicate")}</button>
          <button role="menuitem" onclick={onreuse}><Icon name="plus" size={16} />{t("card.reuse")}</button>
          <button role="menuitem" class="danger" onclick={ondelete}><Icon name="trash" size={16} />{t("card.delete")}</button>
        </div>
      {/if}
    </div>
  </div>
</article>

<style>
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: visible;
    transition: box-shadow 0.15s, transform 0.15s, border-color 0.15s;
  }
  .card:hover {
    box-shadow: var(--shadow);
    border-color: color-mix(in srgb, var(--accent) 35%, var(--border));
    transform: translateY(-1px);
  }
  .thumb {
    display: block;
    width: 100%;
    aspect-ratio: 4 / 3;
    padding: 18px;
    border: none;
    border-bottom: 1px solid var(--border);
    border-radius: var(--radius) var(--radius) 0 0;
    background:
      linear-gradient(var(--plan-grid) 1px, transparent 1px) 0 0 / 16px 16px,
      linear-gradient(90deg, var(--plan-grid) 1px, transparent 1px) 0 0 / 16px 16px,
      var(--surface);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 8px 12px 14px;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  p {
    margin: 2px 0 0;
    font-size: 12.5px;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 36px;
    z-index: 20;
    min-width: 168px;
    padding: 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow-lg);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: 6px;
    background: none;
    text-align: left;
  }
  .menu button:hover {
    background: var(--surface-2);
  }
  .menu .danger {
    color: var(--danger);
  }
</style>
