<script lang="ts">
  // A dialog over the app: takes the focus (its first field, else the dialog
  // itself), keeps Tab / Shift+Tab inside, closes with Escape or a press on
  // the backdrop and gives the focus back to what had it before.
  import { onMount, tick, type Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";

  let {
    title,
    width = 560,
    onclose,
    children,
    footer,
  }: { title: string; width?: number; onclose: () => void; children: Snippet; footer?: Snippet } = $props();

  let dialog: HTMLDivElement | undefined = $state();

  const FOCUSABLE =
    'a[href], button:not(:disabled), input:not(:disabled):not([type="hidden"]), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

  function focusables(): HTMLElement[] {
    if (!dialog) return [];
    return [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null || el === document.activeElement);
  }

  onMount(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    tick().then(() => {
      if (!dialog || dialog.contains(document.activeElement)) return;
      // The body's first control when it is a field to type in (New job:
      // the name), else the dialog itself, so Tab starts at the top.
      const body = dialog.querySelector(".body");
      const first = focusables().find((el) => body?.contains(el));
      const typing = first?.matches('input:not([type="radio"]):not([type="checkbox"]):not([type="button"]), textarea, select');
      (typing ? first! : dialog).focus({ preventScroll: true });
    });
    return () => {
      // Back to the opener when it is still on the page (not when another
      // dialog or the editor took the focus meanwhile).
      const now = document.activeElement;
      if (opener?.isConnected && (!now || now === document.body || dialog?.contains(now))) opener.focus({ preventScroll: true });
    };
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onclose();
      return;
    }
    if (e.key !== "Tab" || !dialog) return;
    const items = focusables();
    if (!items.length) {
      e.preventDefault();
      dialog.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (!dialog.contains(active)) {
      e.preventDefault();
      (e.shiftKey ? last : first).focus();
    } else if (e.shiftKey && (active === first || active === dialog)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" bind:this={dialog} style="width:min({width}px, calc(100vw - 32px))">
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
    outline: none;
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
