<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";

  let {
    title,
    width = 560,
    onclose,
    children,
    footer,
  }: { title: string; width?: number; onclose: () => void; children: Snippet; footer?: Snippet } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={title} style="width:min({width}px, calc(100vw - 32px))">
    <header>
      <h2>{title}</h2>
      <button class="icon-btn" onclick={onclose} aria-label={t("common.close")}><Icon name="x" /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: var(--backdrop);
    backdrop-filter: blur(2px);
  }
  .modal {
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: 14px;
    box-shadow: var(--shadow-lg);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 14px 6px 20px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 650;
  }
  .body {
    padding: 8px 20px 18px;
    overflow: auto;
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 10px;
    padding: 14px 20px;
    border-top: 1px solid var(--border);
  }
  @media (max-height: 560px) {
    .modal {
      max-height: calc(100vh - 16px);
    }
    header {
      padding-top: 8px;
    }
    footer {
      padding: 10px 20px;
    }
  }
</style>
