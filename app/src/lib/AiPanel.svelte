<script lang="ts">
  // Everything AI agents did to this design: what they are doing right now,
  // and each saved change with its summary and the exact edits it made.
  import Icon from "./Icon.svelte";
  import { describeChange } from "./changes";
  import { relativeTime } from "./format";
  import { t } from "./i18n.svelte";
  import { actorName, isFresh, live } from "./live.svelte";
  import type { ActivityItem, Change, Project } from "./types";
  import { isAi } from "./types";

  let {
    id,
    project,
    activity,
    onrevert,
    onconnect,
    onfocus,
  }: {
    id: string;
    project: Project;
    activity: ActivityItem[];
    onrevert: () => void;
    onconnect: () => void;
    onfocus: (c: Change | null) => void;
  } = $props();

  let open = $state<Record<number, boolean>>({});
  const here = $derived(isFresh() && live.status?.project_id === id);
</script>

<div class="panel">
  <div class="live" class:active={isFresh()}>
    <span class="dot"></span>
    {#if isFresh() && live.status}
      <div>
        <strong>{actorName(live.status.actor)}</strong>
        <span>{live.status.message}</span>
        {#if !here && live.status.project_id}<em>{t("ai.otherProject")}</em>{/if}
      </div>
    {:else}
      <div>
        <strong>{t("ai.idle")}</strong>
        <span>{t("ai.idleHint")}</span>
      </div>
    {/if}
  </div>
  <button class="btn connect" onclick={onconnect}><Icon name="link" size={15} />{t("ai.connect")}</button>

  <h4>{t("ai.history")}</h4>
  {#if activity.length === 0}
    <p class="empty">{t("ai.noHistory")}</p>
  {/if}
  <ol>
    {#each activity as a, i (a.ts_ms)}
      {@const ai = isAi(a.actor)}
      <li class:ai>
        <div class="head">
          <span class="who" class:ai><Icon name={ai ? "sparkle" : "cursor"} size={13} />{ai ? actorName(a.actor) : t("ai.you")}</span>
          <time>{relativeTime(a.ts_ms)}</time>
        </div>
        <p class="summary">{!ai && a.summary.startsWith("Reverted") ? t("ai.revertedEntry") : a.summary}</p>
        {#if a.changes.length}
          <button class="link" onclick={() => (open[i] = !open[i])}>
            {open[i] ? t("ai.hideChanges") : t("ai.showChanges", { n: a.changes.length })}
          </button>
          {#if open[i]}
            <ul>
              {#each a.changes as c}
                <li onpointerenter={() => onfocus(c)} onpointerleave={() => onfocus(null)}>
                  {describeChange(c, project)}
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
        {#if i === 0 && ai}
          <button class="btn sm" onclick={onrevert}><Icon name="undo" size={14} />{t("ai.revert")}</button>
        {/if}
      </li>
    {/each}
  </ol>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .live {
    display: flex;
    gap: 10px;
    padding: 12px;
    border-radius: 10px;
    background: var(--surface-2);
    font-size: 13px;
  }
  .live.active {
    background: var(--ai-soft);
  }
  .live div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .live span {
    color: var(--text-2);
  }
  .live em {
    font-style: normal;
    font-size: 12px;
    color: var(--text-3);
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    margin-top: 5px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .live.active .dot {
    background: var(--ai);
    box-shadow: 0 0 0 0 var(--ai);
    animation: ping 1.4s infinite;
  }
  @keyframes ping {
    70% {
      box-shadow: 0 0 0 7px transparent;
    }
  }
  .connect {
    justify-content: center;
  }
  h4 {
    margin: 8px 0 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-3);
  }
  .empty {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  ol > li {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  ol > li.ai {
    border-color: color-mix(in srgb, var(--ai) 35%, var(--border));
  }
  .head {
    display: flex;
    justify-content: space-between;
    width: 100%;
    font-size: 12px;
    color: var(--text-3);
  }
  .who {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 600;
    color: var(--text-2);
  }
  .who.ai {
    color: var(--ai-strong);
  }
  .summary {
    margin: 0;
    font-size: 13px;
  }
  .link {
    padding: 0;
    border: none;
    background: none;
    font-size: 12.5px;
    color: var(--focus);
  }
  ul {
    margin: 0;
    padding-left: 16px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  ul li {
    padding: 1px 0;
  }
  ul li:hover {
    color: var(--text);
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
</style>
