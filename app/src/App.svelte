<script lang="ts">
  import { onMount } from "svelte";
  import AiConnect from "./lib/AiConnect.svelte";
  import Editor from "./lib/Editor.svelte";
  import Home from "./lib/Home.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import Toasts from "./lib/Toasts.svelte";
  import { api, onLiveStatus, onOpenFailed, onOpenProject, onProjectsChanged, onSettingsChanged } from "./lib/api";
  import { t } from "./lib/i18n.svelte";
  import { actorName, live } from "./lib/live.svelte";
  import { info, notify, notifyError } from "./lib/notify.svelte";
  import { applySettings, loadSettings, prefs } from "./lib/settings.svelte";
  import { hasUnsaved, isBusy, mayLeave } from "./lib/leave";
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

  /**
   * Newest status follow mode has seen. Only statuses written after the app
   * started are followed; the start-up read must not mark a status that is
   * still on its way as an event as seen (a race that kept the app from
   * following under load).
   */
  let followSeen = 0;
  const startedAt = Date.now();

  /**
   * Follow mode: open the job the AI agent saved or asked to show. Reading
   * or calculating a job never switches; while the user draws, places an
   * object, types or has a dialog open, a toast offers to open it instead
   * (UX-001).
   */
  function onLive(s: LiveStatus) {
    const fresh = s.ts_ms > followSeen;
    followSeen = Math.max(followSeen, s.ts_ms);
    if (!live.status || s.ts_ms >= live.status.ts_ms) live.status = s;
    live.now = Date.now();
    if (!fresh || !s.follow || !s.project_id || !prefs.followAi || s.project_id === openId) return;
    const next = s.project_id;
    const who = actorName(s.actor);
    if (isBusy()) {
      notify({
        kind: "info",
        title: t("ai.followOffer", { who }),
        action: { label: t("ai.followOpen"), run: () => openJob(next) },
      });
      return;
    }
    // Follow mode never drops edits that could not be saved; the user stays.
    mayLeave("quiet").then((ok) => {
      if (!ok || openId === next) return;
      openId = next;
      info(t("ai.followed", { who }));
    });
  }

  /** Opens another job (or the library, with null) once the open one may be left. */
  async function openJob(id: string | null) {
    if (id === openId || !(await mayLeave())) return;
    openId = id;
    if (id === null) refresh();
  }

  /** Last resort: anything that slipped through still reaches the user. */
  function onUnexpected(e: unknown) {
    notifyError(e);
  }

  /** The projects folder changed: reload the library from the new one. The
   * open job belongs to the old folder, so the editor closes (ERR-019). */
  async function libraryChanged() {
    const d = await api.projectsDir().catch(() => dir);
    if (dir && d !== dir) openId = null;
    dir = d;
    refresh();
  }

  async function closeGuard(): Promise<() => void> {
    if ((window as unknown as { __TECTORA_PREVIEW__?: boolean }).__TECTORA_PREVIEW__) {
      // Browser preview: a page unload cannot wait for a save, only warn.
      const onUnload = (e: BeforeUnloadEvent) => {
        if (hasUnsaved()) e.preventDefault();
      };
      window.addEventListener("beforeunload", onUnload);
      return () => window.removeEventListener("beforeunload", onUnload);
    }
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      return await getCurrentWindow().onCloseRequested(async (e) => {
        if (!(await mayLeave())) e.preventDefault();
      });
    } catch {
      return () => {};
    }
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
    api.liveStatus().then(
      (s) => {
        if (!s) return;
        if (!live.status || s.ts_ms > live.status.ts_ms) live.status = s;
        if (s.ts_ms < startedAt) followSeen = Math.max(followSeen, s.ts_ms);
      },
      () => {},
    );
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
      openJob(id);
    };
    // Closing the window: the open job is saved first, and edits that cannot
    // be saved are only dropped when the user says so.
    const unClose = closeGuard();
    api.takePendingOpen().then((id) => id && openFile(id));
    const unOpen = onOpenProject(openFile);
    // A file opened from the system that did not open (damaged, newer, not
    // a job): told like a failed import, at start-up and later (ERR-006).
    const showOpenErrors = () =>
      api.takeOpenErrors().then(
        (es) => es.forEach((e) => notifyError(e, "import")),
        () => {},
      );
    const unOpenFailed = onOpenFailed(showOpenErrors);
    showOpenErrors();
    const tick = setInterval(() => (live.now = Date.now()), 5000);
    return () => {
      un.then((f) => f());
      unLive.then((f) => f());
      unSettings.then((f) => f());
      unOpen.then((f) => f());
      unOpenFailed.then((f) => f());
      unClose.then((f) => f());
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
      onback={() => openJob(null)}
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
