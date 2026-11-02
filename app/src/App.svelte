<script lang="ts">
  import { onMount } from "svelte";
  import AiConnect from "./lib/AiConnect.svelte";
  import Editor from "./lib/Editor.svelte";
  import Home from "./lib/Home.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import { api, onLiveStatus, onProjectsChanged, onSettingsChanged } from "./lib/api";
  import { t } from "./lib/i18n.svelte";
  import { actorName, live } from "./lib/live.svelte";
  import { applySettings, loadSettings, prefs } from "./lib/settings.svelte";
  import type { ExampleInfo, LiveStatus, ProjectSummary } from "./lib/types";

  let openId = $state<string | null>(null);
  let projects = $state<ProjectSummary[]>([]);
  let examples = $state<ExampleInfo[]>([]);
  let dir = $state("");
  let connect = $state(false);
  let settingsOpen = $state(false);
  let toast = $state<{ text: string; error: boolean } | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  function show(text: string, error = false) {
    toast = { text, error };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), error ? 6000 : 4000);
  }

  async function refresh() {
    try {
      projects = await api.listProjects();
    } catch (e) {
      show(String(e), true);
    }
  }

  /** Follow mode: open the design the AI agent is working on. */
  function onLive(s: LiveStatus) {
    const fresh = !live.status || s.ts_ms > live.status.ts_ms;
    live.status = s;
    live.now = Date.now();
    if (!fresh || !s.project_id || !prefs.followAi || s.project_id === openId) return;
    openId = s.project_id;
    show(t("ai.followed", { who: actorName(s.actor) }));
  }

  /** The projects folder changed: reload the library from the new one. */
  function libraryChanged() {
    api.projectsDir().then((d) => (dir = d));
    refresh();
  }

  onMount(() => {
    loadSettings().catch(() => {});
    refresh();
    api.listExamples().then((x) => (examples = x));
    api.projectsDir().then((d) => (dir = d));
    api.liveStatus().then((s) => (live.status = s));
    // New or changed designs from the CLI / AI agents appear immediately.
    const un = onProjectsChanged(() => refresh());
    const unLive = onLiveStatus(onLive);
    // Settings changed outside this window (e.g. by an AI agent over MCP).
    const unSettings = onSettingsChanged((v) => {
      const moved = v.effective.projects_dir !== dir;
      applySettings(v);
      if (moved) libraryChanged();
    });
    const tick = setInterval(() => (live.now = Date.now()), 5000);
    return () => {
      un.then((f) => f());
      unLive.then((f) => f());
      unSettings.then((f) => f());
      clearInterval(tick);
    };
  });
</script>

<svelte:window
  onkeydown={(e) => {
    // Cmd+, / Ctrl+, opens the settings, as in most desktop apps.
    if ((e.metaKey || e.ctrlKey) && e.key === ",") {
      e.preventDefault();
      settingsOpen = true;
    }
  }}
/>

{#if openId}
  {#key openId}
    <Editor
      id={openId}
      onback={() => {
        openId = null;
        refresh();
      }}
      onerror={(m) => show(m, true)}
      onconnect={() => (connect = true)}
    />
  {/key}
{:else}
  <Home
    {projects}
    {examples}
    {dir}
    onopen={(id) => (openId = id)}
    onchanged={refresh}
    onerror={(m) => show(m, true)}
    onconnect={() => (connect = true)}
    onsettings={() => (settingsOpen = true)}
  />
{/if}

{#if settingsOpen}
  <SettingsDialog onclose={() => (settingsOpen = false)} onlibrarychanged={libraryChanged} />
{/if}

{#if connect}
  <AiConnect onclose={() => (connect = false)} />
{/if}

{#if toast}
  <div class="toast" class:error={toast.error} role="status">{toast.text}</div>
{/if}

<style>
  .toast {
    position: fixed;
    left: 50%;
    bottom: 40px;
    transform: translateX(-50%);
    max-width: 560px;
    padding: 10px 16px;
    border-radius: 10px;
    background: var(--text);
    color: var(--surface);
    box-shadow: var(--shadow-lg);
    font-size: 13px;
    z-index: 100;
  }
  .toast.error {
    background: var(--danger);
    color: #fff;
  }
</style>
