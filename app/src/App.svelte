<script lang="ts">
  import { onMount } from "svelte";
  import AiConnect from "./lib/AiConnect.svelte";
  import Editor from "./lib/Editor.svelte";
  import Home from "./lib/Home.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import Toasts from "./lib/Toasts.svelte";
  import { api, onLiveStatus, onOpenProject, onProjectsChanged, onSettingsChanged } from "./lib/api";
  import { t } from "./lib/i18n.svelte";
  import { actorName, live } from "./lib/live.svelte";
  import { info, notifyError } from "./lib/notify.svelte";
  import { applySettings, loadSettings, prefs } from "./lib/settings.svelte";
  import type { BrokenFile, ExampleInfo, LiveStatus, ProjectSummary } from "./lib/types";

  let openId = $state<string | null>(null);
  let projects = $state<ProjectSummary[]>([]);
  let broken = $state<BrokenFile[]>([]);
  let examples = $state<ExampleInfo[]>([]);
  let dir = $state("");
  let connect = $state(false);
  let settingsOpen = $state(false);

  async function refresh() {
    try {
      const lib = await api.listProjects();
      projects = lib.projects;
      broken = lib.broken;
    } catch (e) {
      notifyError(e, "list");
    }
  }

  /** Follow mode: open the design the AI agent is working on. */
  function onLive(s: LiveStatus) {
    const fresh = !live.status || s.ts_ms > live.status.ts_ms;
    live.status = s;
    live.now = Date.now();
    if (!fresh || !s.project_id || !prefs.followAi || s.project_id === openId) return;
    openId = s.project_id;
    info(t("ai.followed", { who: actorName(s.actor) }));
  }

  /** Last resort: anything that slipped through still reaches the user. */
  function onUnexpected(e: unknown) {
    notifyError(e);
  }

  /** The projects folder changed: reload the library from the new one. */
  function libraryChanged() {
    api.projectsDir().then((d) => (dir = d));
    refresh();
  }

  onMount(() => {
    const onError = (e: ErrorEvent) => onUnexpected(e.error ?? e.message);
    const onRejection = (e: PromiseRejectionEvent) => onUnexpected(e.reason);
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    loadSettings().catch(() => {});
    refresh();
    api.listExamples().then((x) => (examples = x), (e) => notifyError(e, "list"));
    api.projectsDir().then((d) => (dir = d), () => {});
    api.liveStatus().then((s) => (live.status = s), () => {});
    api.startupWarnings().then(
      (ws) => ws.forEach((w) => notifyError(w)),
      () => {},
    );
    // New or changed designs from the CLI / AI agents appear immediately.
    const un = onProjectsChanged(() => refresh());
    const unLive = onLiveStatus(onLive);
    // Settings changed outside this window (e.g. by an AI agent over MCP).
    const unSettings = onSettingsChanged((v) => {
      const moved = v.effective.projects_dir !== dir;
      applySettings(v);
      if (moved) libraryChanged();
    });
    // A .tectora / .tectorajob file opened from the system (double-click).
    const openFile = (id: string) => {
      refresh();
      openId = id;
    };
    api.takePendingOpen().then((id) => id && openFile(id));
    const unOpen = onOpenProject(openFile);
    const tick = setInterval(() => (live.now = Date.now()), 5000);
    return () => {
      un.then((f) => f());
      unLive.then((f) => f());
      unSettings.then((f) => f());
      unOpen.then((f) => f());
      clearInterval(tick);
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
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
    {broken}
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

<Toasts />
