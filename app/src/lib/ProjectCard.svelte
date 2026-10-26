<script lang="ts">
  import Icon from "./Icon.svelte";
  import PlanThumb from "./PlanThumb.svelte";
  import { m2, relativeTime } from "./format";
  import type { ProjectSummary } from "./types";

  let {
    project,
    onopen,
    onduplicate,
    ondelete,
  }: {
    project: ProjectSummary;
    onopen: () => void;
    onduplicate: () => void;
    ondelete: () => void;
  } = $props();

  let menuOpen = $state(false);
</script>

<svelte:window onclick={() => (menuOpen = false)} />

<article class="card" ondblclick={onopen}>
  <button class="thumb" onclick={onopen} aria-label="Otevřít {project.name}">
    <PlanThumb outlines={project.outlines} />
  </button>
  <div class="meta">
    <div class="text">
      <h3 title={project.name}>{project.name}</h3>
      <p class="muted">{relativeTime(project.modified_ms)} · {m2(project.roof_area)}</p>
    </div>
    <div class="menu-wrap">
      <button
        class="icon-btn"
        aria-label="Další akce"
        onclick={(e) => {
          e.stopPropagation();
          menuOpen = !menuOpen;
        }}
      >
        <Icon name="more" />
      </button>
      {#if menuOpen}
        <div class="menu" role="menu">
          <button role="menuitem" onclick={onopen}><Icon name="open" size={16} />Otevřít</button>
          <button role="menuitem" onclick={onduplicate}><Icon name="copy" size={16} />Duplikovat</button>
          <button role="menuitem" class="danger" onclick={ondelete}><Icon name="trash" size={16} />Smazat</button>
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
