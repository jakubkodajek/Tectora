<script lang="ts">
  // Connects AI apps to this app's built-in MCP server: one click for the
  // apps Tectora can add itself to (tectora_core::assistants), copy-paste
  // settings for any other app that runs local MCP servers.
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { describeError } from "./errors";
  import { t } from "./i18n.svelte";
  import { tabIds, tablist } from "./tabs";
  import { prefs, savePrefs, updateSettings } from "./settings.svelte";
  import type { AiAssistant, AiAssistantId } from "./types";

  let { onclose }: { onclose: () => void } = $props();
  const ids = tabIds("connect");

  type Other = "claudeCode" | "json" | "chatgpt";
  let other = $state<Other>("claudeCode");
  let cmd = $state<{ command: string; args: string[] }>({ command: "tectora-app", args: ["mcp"] });
  let apps = $state<AiAssistant[] | null>(null);
  let busy = $state<AiAssistantId | null>(null);
  /** The app just connected, whose restart hint is shown. */
  let done = $state<AiAssistantId | null>(null);
  let failed = $state<{ id: AiAssistantId; message: string; detail?: string } | null>(null);
  let copied = $state(false);

  async function refresh() {
    try {
      apps = await api.aiAssistants();
    } catch (e) {
      // Without the list only the copy-paste settings below are offered.
      console.error("[tectora] ai_assistants:", e);
      apps = [];
    }
  }

  onMount(() => {
    // On failure the default command stays; it works when the app is on PATH.
    api.mcpCommand().then((c) => (cmd = c), () => {});
    refresh();
  });

  async function connect(id: AiAssistantId) {
    busy = id;
    failed = null;
    done = null;
    try {
      await api.connectAiAssistant(id);
      done = id;
    } catch (e) {
      const d = describeError(e, "connect");
      failed = { id, message: d.message, detail: d.detail };
    } finally {
      busy = null;
      await refresh();
    }
  }

  const json = $derived(JSON.stringify({ mcpServers: { tectora: { command: cmd.command, args: cmd.args } } }, null, 2));
  const snippet = $derived(other === "claudeCode" ? `claude mcp add tectora -- "${cmd.command}" ${cmd.args.join(" ")}` : other === "json" ? json : "");

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

  <h4>{t("conn.apps")}</h4>
  {#if apps === null}
    <p class="step muted">{t("conn.loading")}</p>
  {:else if apps.length}
    <ul class="apps">
      {#each apps as app (app.id)}
        <li>
          <div class="row">
            <span class="name">{app.name}</span>
            <span class="state" class:ok={app.state === "connected"} data-state={app.state}>
              {#if app.state === "connected"}<Icon name="check" size={13} />{:else}<span class="mark" aria-hidden="true"></span>{/if}{t(`conn.state.${app.state}`)}
            </span>
            {#if app.state !== "not_installed"}
              <button
                class="btn sm"
                class:primary={app.state !== "connected"}
                disabled={busy !== null}
                onclick={() => connect(app.id)}
                title={app.files.join("\n")}
              >
                {busy === app.id ? t("conn.connecting") : app.state === "connected" ? t("conn.reconnect") : app.state === "outdated" ? t("conn.fix") : t("conn.connect")}
              </button>
            {/if}
          </div>
          {#if done === app.id}
            <p class="hint">{t(`conn.restart.${app.id}`)}</p>
          {/if}
          {#if failed?.id === app.id}
            <div class="err" role="alert">
              <span class="mark bad" aria-hidden="true"></span>
              <span>{failed.message}</span>
              {#if failed.detail}<code>{failed.detail}</code>{/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  <h4>{t("conn.other")}</h4>
  <div class="tabs" role="tablist" use:tablist>
    {#each [["claudeCode", "Claude Code"], ["json", t("conn.otherJson")], ["chatgpt", "ChatGPT"]] as [id, label]}
      <button role="tab" id={ids.tab(id)} aria-controls={ids.panel} aria-selected={other === id} class:on={other === id} onclick={() => (other = id as Other)}>{label}</button>
    {/each}
  </div>
  <div role="tabpanel" id={ids.panel} aria-labelledby={ids.tab(other)}>
  {#if other === "chatgpt"}
    <p class="note">{t("conn.chatgpt")}</p>
  {:else}
    <p class="step">{other === "claudeCode" ? t("conn.terminal") : t("conn.jsonWhere")}</p>
    <div class="code">
      <pre>{snippet}</pre>
      <button class="btn sm" onclick={copy}><Icon name={copied ? "check" : "copy"} size={14} />{copied ? t("conn.copied") : t("conn.copy")}</button>
    </div>
  {/if}
  </div>

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
    margin: 0 0 6px;
    color: var(--text-2);
  }
  .apps {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  .apps li {
    padding: 8px 12px;
  }
  .apps li + li {
    border-top: 1px solid var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 30px;
  }
  .name {
    font-weight: 500;
    font-size: 13px;
    min-width: 120px;
  }
  .state {
    flex: 1;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .mark {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: none;
    background: var(--text-3);
  }
  .state.ok {
    color: var(--text);
  }
  .state[data-state="outdated"] .mark,
  .mark.bad {
    background: var(--danger);
  }
  .hint {
    margin: 6px 0 2px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .err {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 6px 0 2px;
    font-size: 12.5px;
  }
  .err code {
    flex-basis: 100%;
    font: 11.5px/1.4 ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    color: var(--text-2);
    word-break: break-all;
    user-select: text;
  }
  .muted {
    color: var(--text-2);
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 3px;
    border-radius: 9px;
    background: var(--surface-2);
    margin-bottom: 8px;
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
    padding-right: 110px;
    border-radius: 8px;
    background: var(--code-bg);
    color: var(--code-text);
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
