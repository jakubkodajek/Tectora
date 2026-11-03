<script lang="ts">
  // How to connect an AI client to this app's built-in MCP server.
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { t } from "./i18n.svelte";
  import { prefs, savePrefs, updateSettings } from "./settings.svelte";

  let { onclose }: { onclose: () => void } = $props();

  type Client = "claude" | "claudeCode" | "gemini" | "chatgpt";
  let client = $state<Client>("claude");
  let cmd = $state<{ command: string; args: string[] }>({ command: "tectora-app", args: ["mcp"] });
  let copied = $state(false);
  const isWin = navigator.userAgent.includes("Windows");

  onMount(() => {
    // On failure the default command stays; it works when the app is on PATH.
    api.mcpCommand().then((c) => (cmd = c), () => {});
  });

  const json = $derived(JSON.stringify({ mcpServers: { tectora: { command: cmd.command, args: cmd.args } } }, null, 2));
  const snippet = $derived(
    client === "claudeCode" ? `claude mcp add tectora -- "${cmd.command}" ${cmd.args.join(" ")}` : client === "chatgpt" ? "" : json,
  );
  const where = $derived(
    client === "claude"
      ? isWin
        ? "%APPDATA%\\Claude\\claude_desktop_config.json"
        : "~/Library/Application Support/Claude/claude_desktop_config.json"
      : client === "gemini"
        ? "~/.gemini/settings.json"
        : "",
  );

  async function copy() {
    try {
      await navigator.clipboard.writeText(snippet);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard blocked; the text stays selectable.
    }
  }
</script>

<Modal title={t("conn.title")} width={620} {onclose}>
  <p class="lead">{t("conn.lead")}</p>
  <div class="tabs" role="tablist">
    {#each [["claude", "Claude Desktop"], ["claudeCode", "Claude Code"], ["gemini", "Gemini CLI"], ["chatgpt", "ChatGPT"]] as [id, label]}
      <button role="tab" aria-selected={client === id} class:on={client === id} onclick={() => (client = id as Client)}>{label}</button>
    {/each}
  </div>

  {#if client === "chatgpt"}
    <p class="note">{t("conn.chatgpt")}</p>
  {:else}
    <p class="step">{client === "claudeCode" ? t("conn.terminal") : t("conn.file", { file: where })}</p>
    <div class="code">
      <pre>{snippet}</pre>
      <button class="btn sm" onclick={copy}><Icon name={copied ? "check" : "copy"} size={14} />{copied ? t("conn.copied") : t("conn.copy")}</button>
    </div>
    <p class="step">{t("conn.restart")}</p>
  {/if}

  <h4>{t("conn.howTitle")}</h4>
  <ol class="how">
    <li>{t("conn.how1")}</li>
    <li>{t("conn.how2")}</li>
    <li>{t("conn.how3")}</li>
  </ol>
  <label class="check">
    <input type="checkbox" bind:checked={prefs.followAi} onchange={() => { savePrefs(); updateSettings({ follow_ai: prefs.followAi }).catch(() => {}); }} />
    {t("conn.follow")}
  </label>
</Modal>

<style>
  .lead {
    margin: 0 0 14px;
    color: var(--text-2);
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 3px;
    border-radius: 9px;
    background: var(--surface-2);
    margin-bottom: 12px;
  }
  .tabs button {
    flex: 1;
    height: 30px;
    border: none;
    border-radius: 7px;
    background: transparent;
    font-weight: 500;
    font-size: 13px;
    color: var(--text-2);
  }
  .tabs button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  .step,
  .note {
    margin: 8px 0;
    font-size: 13px;
  }
  .note {
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text-2);
  }
  .code {
    position: relative;
  }
  pre {
    margin: 0;
    padding: 12px 14px;
    border-radius: 8px;
    background: #12151d;
    color: #e6e9f0;
    font: 12.5px/1.5 ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    white-space: pre-wrap;
    word-break: break-all;
    user-select: text;
  }
  .code .btn {
    position: absolute;
    top: 8px;
    right: 8px;
    height: 28px;
    font-size: 12px;
  }
  h4 {
    margin: 16px 0 6px;
    font-size: 13px;
  }
  .how {
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
    color: var(--text-2);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
    font-size: 13px;
  }
</style>
