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
      <div class="head">
        <i class="dot"></i>
        <strong>{toast.title}</strong>
        {#if toast.count > 1}<span class="mono">×{toast.count}</span>{/if}
        <button class="close" onclick={() => dismiss(toast.id)} aria-label={t("err.close")} title={t("err.close")}><Icon name="x" size={14} /></button>
      </div>
      {#if toast.message}<p>{toast.message}</p>{/if}
      {#if toast.code || toast.detail}
        <div class="foot">
          {#if toast.code}<span class="mono">{toast.code}</span>{/if}
          {#if toast.detail}
            <button class="link" onclick={() => (open = open === toast.id ? null : toast.id)} aria-expanded={open === toast.id}>
              {open === toast.id ? t("err.hideDetails") : t("err.details")}
            </button>
            {#if open === toast.id}
              <button class="link" onclick={() => copy(toast.id, toast.detail ?? "")}>{copied === toast.id ? t("err.copied") : t("err.copy")}</button>
            {/if}
          {/if}
        </div>
        {#if open === toast.id && toast.detail}<pre>{toast.detail}</pre>{/if}
      {/if}
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    right: 16px;
    bottom: 40px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
    z-index: 100;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    width: min(380px, calc(100vw - 32px));
    padding: 10px 12px;
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    box-shadow: 0 4px 16px rgba(20, 24, 36, 0.08);
    font-size: 13px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 1px;
    background: var(--text-3);
  }
  .error .dot {
    background: var(--danger);
  }
  .warning .dot {
    background: var(--ai);
  }
  .info .dot {
    background: #2c9a5b;
  }
  strong {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  p {
    margin: 4px 0 0 14px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 8px 0 0 14px;
  }
  .mono,
  pre {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11px;
    color: var(--text-3);
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .link:hover {
    color: var(--text);
    text-decoration: underline;
  }
  pre {
    margin: 6px 0 0 14px;
    padding: 6px 8px;
    max-height: 120px;
    overflow: auto;
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .close {
    display: flex;
    padding: 2px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--text-3);
    cursor: pointer;
  }
  .close:hover {
    color: var(--text);
  }
</style>
