<script lang="ts">
  import { onMount } from "svelte";
  import Editor from "./lib/Editor.svelte";
  import Home from "./lib/Home.svelte";
  import { api, onProjectsChanged } from "./lib/api";
  import type { ExampleInfo, ProjectSummary } from "./lib/types";

  let openId = $state<string | null>(null);
  let projects = $state<ProjectSummary[]>([]);
  let examples = $state<ExampleInfo[]>([]);
  let dir = $state("");
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

  onMount(() => {
    refresh();
    api.listExamples().then((x) => (examples = x));
    api.projectsDir().then((d) => (dir = d));
    // New or changed designs from the CLI / AI agents appear immediately.
    const un = onProjectsChanged(() => refresh());
    return () => {
      un.then((f) => f());
    };
  });
</script>

{#if openId}
  {#key openId}
    <Editor
      id={openId}
      onback={() => {
        openId = null;
        refresh();
      }}
      onerror={(m) => show(m, true)}
      onnotice={(m) => show(m)}
    />
  {/key}
{:else}
  <Home {projects} {examples} {dir} onopen={(id) => (openId = id)} onchanged={refresh} onerror={(m) => show(m, true)} />
{/if}

{#if toast}
  <div class="toast" class:error={toast.error} role="status">{toast.text}</div>
{/if}

<style>
  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
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
