<script lang="ts">
  // Everything AI agents did to this design: what they are doing right now,
  // and each saved change with its summary and the exact edits it made.
  import Icon from "./Icon.svelte";
  import { describeChange } from "./changes";
  import { relativeTime } from "./format";
  import { t } from "./i18n.svelte";
  import { actorName, entrySummary, isFresh, isRevert, live, liveText } from "./live.svelte";
  import { app as appSettings } from "./settings.svelte";
  import { changeRows, changeState, rowLabel } from "./settingschange";
  import type { ActivityItem, Change, Project } from "./types";
  import { isAi } from "./types";

  let {
    id,
    project,
    activity,
    onrevert,
    onconnect,
    onfocus,
    busy = false,
  }: {
    id: string;
    project: Project;
    activity: ActivityItem[];
    onrevert: () => void;
    onconnect: () => void;
    onfocus: (c: Change | null) => void;
    /** A revert or accept is running: the button waits for it. */
    busy?: boolean;
  } = $props();

  let open = $state<Record<number, boolean>>({});
  const here = $derived(isFresh() && live.status?.project_id === id);
  // App settings AI clients changed (UX-003), newest first; an open one is
  // answered on the card App.svelte shows.
  const settingsChanges = $derived([...(appSettings.view?.settings.ai_changes ?? [])].reverse().slice(0, 5));
</script>

<div class="panel">
  <div class="live" class:active={isFresh()}>
    <span class="dot"></span>
    {#if isFresh() && live.status}
      <div>
        <strong>{actorName(live.status.actor)}</strong>
        <span>{liveText(live.status)}</span>
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
    <!-- Two entries can share a millisecond (two writers, a clock step): the
         index keeps the key unique (ERRU-009). -->
    {#each activity as a, i (`${a.ts_ms}:${i}`)}
      {@const ai = isAi(a.actor)}
      <li class:ai>
        <div class="head">
          <span class="who" class:ai><Icon name={ai ? "sparkle" : "cursor"} size={13} />{ai ? actorName(a.actor) : t("ai.you")}</span>
          <time>{relativeTime(a.ts_ms)}</time>
        </div>
        <p class="summary">{!ai && isRevert(a.summary) ? t("ai.revertedEntry") : entrySummary(a.summary)}</p>
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
          <button class="btn sm" disabled={busy} onclick={onrevert}><Icon name="undo" size={14} />{t("ai.revert")}</button>
        {/if}
      </li>
    {/each}
  </ol>

  {#if settingsChanges.length}
    <h4>{t("aiChange.history")}</h4>
    <ol>
      {#each settingsChanges as c (c.id)}
        <li class="ai">
          <div class="head">
            <span class="who ai"><Icon name="sparkle" size={13} />{actorName(c.by)}</span>
            <time>{relativeTime(c.id)}</time>
          </div>
          <p class="summary">{c.summary}</p>
          <ul>
            {#each changeRows(c) as r (r.key)}
              <!-- As the card names it, one colon only: "New roofs: Overhang 0.50 m → 0.70 m" (UX-302). -->
              <li data-testid="aichange-row">{rowLabel(r)} {r.before} → {r.after}</li>
            {/each}
          </ul>
          <span class="state">{changeState(c)}</span>
        </li>
      {/each}
    </ol>
  {/if}
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
  .state {
    font-size: 12px;
    color: var(--text-3);
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
</style>
