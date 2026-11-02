<script lang="ts">
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { dismiss, hold, toasts } from "./notify.svelte";

  let open = $state<number | null>(null);
  let copied = $state<number | null>(null);

  async function copy(id: number, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = id;
      setTimeout(() => (copied = null), 1500);
    } catch {
      // Clipboard blocked; the text stays selectable.
    }
  }
</script>

<div class="stack" aria-live="polite">
  {#each toasts as toast (toast.id)}
    <div
      class="toast {toast.kind}"
      role={toast.kind === "error" ? "alert" : "status"}
      onmouseenter={() => hold(toast.id, true)}
      onmouseleave={() => hold(toast.id, false)}
    >
      <span class="icon"><Icon name={toast.kind === "info" ? "check" : "warning"} size={18} /></span>
      <div class="body">
        <strong>{toast.title}{#if toast.count > 1}<span class="count">×{toast.count}</span>{/if}</strong>
        {#if toast.message}<p>{toast.message}</p>{/if}
        {#if toast.detail}
          <button class="link" onclick={() => (open = open === toast.id ? null : toast.id)} aria-expanded={open === toast.id}>
            {open === toast.id ? t("err.hideDetails") : t("err.details")}
          </button>
          {#if open === toast.id}
            <div class="detail">
              <code>{toast.detail}</code>
              <button class="link" onclick={() => copy(toast.id, toast.detail ?? "")}>{copied === toast.id ? t("err.copied") : t("err.copy")}</button>
            </div>
          {/if}
        {/if}
      </div>
      <button class="close" onclick={() => dismiss(toast.id)} aria-label={t("err.close")} title={t("err.close")}><Icon name="x" size={16} /></button>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    right: 20px;
    bottom: 44px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 10px;
    z-index: 100;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: start;
    gap: 10px;
    width: min(420px, calc(100vw - 40px));
    padding: 12px 10px 12px 14px;
    border-radius: 10px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    border-left: 4px solid var(--text-3);
    box-shadow: var(--shadow-lg);
    font-size: 13px;
    animation: rise 0.18s ease-out;
  }
  .toast.info {
    border-left-color: #2c9a5b;
  }
  .toast.info .icon {
    color: #2c9a5b;
  }
  .toast.warning {
    border-left-color: var(--ai);
  }
  .toast.warning .icon {
    color: var(--ai-strong);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  .toast.error .icon {
    color: var(--danger);
  }
  .icon {
    display: flex;
    padding-top: 1px;
  }
  .body {
    min-width: 0;
  }
  strong {
    display: block;
    font-weight: 600;
    line-height: 1.35;
  }
  .count {
    margin-left: 6px;
    font-weight: 500;
    color: var(--text-3);
  }
  p {
    margin: 3px 0 0;
    color: var(--text-2);
    line-height: 1.45;
  }
  .link {
    margin-top: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: 12px;
    text-decoration: underline;
    cursor: pointer;
  }
  .link:hover {
    color: var(--text);
  }
  .detail {
    margin-top: 6px;
    padding: 8px;
    border-radius: 6px;
    background: var(--surface-2);
  }
  code {
    display: block;
    max-height: 120px;
    overflow: auto;
    font-size: 11.5px;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .close {
    display: flex;
    padding: 4px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-3);
    cursor: pointer;
  }
  .close:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
</style>
