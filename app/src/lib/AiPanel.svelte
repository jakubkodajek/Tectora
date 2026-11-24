<script lang="ts">
  // Everything AI agents did to this design: what they are doing right now,
  // and each saved change with its summary and the exact edits it made.
  import Icon from "./Icon.svelte";
  import { tick } from "svelte";
  import { describeChange, describeFigure } from "./changes";
  import { relativeTime } from "./format";
  import { t } from "./i18n.svelte";
  import { actorLabel, actorName, entryKind, historyText, isFresh, live, liveText } from "./live.svelte";
  import { app as appSettings } from "./settings.svelte";
  import { pressKeepingFocus } from "./press";
  import { changeRows, changeState, rowLabel } from "./settingschange";
  import type { ActivityItem, Change, Project, Recalculation } from "./types";
  import { ENGINE_ACTOR, byAUser, isAi } from "./types";

  let {
    id,
    project,
    activity,
    recalculation = null,
    onrevert,
    onrestore,
    onconnect,
    onfocus,
    busy = false,
    shown = $bindable(null),
  }: {
    id: string;
    project: Project;
    activity: ActivityItem[];
    /** The engine's recalculation the job's stored result still is (core `Store::recalculation`), if any. */
    recalculation?: Recalculation | null;
    /** Reverts the latest change, the entry at `ts` (the one the button is drawn on). */
    onrevert: (ts: number) => void | Promise<void>;
    /** Puts the offer's own figures back in place of the recalculation logged at `ts`. */
    onrestore: (ts: number) => void | Promise<void>;
    onconnect: () => void;
    onfocus: (c: Change | null) => void;
    /** A revert or accept is running: the button waits for it. */
    busy?: boolean;
    /** An entry to open and scroll to (its `ts_ms`); taken once. */
    shown?: number | null;
  } = $props();

  /**
   * Which entries show their changes, by entry (UI-804): keyed by position,
   * an opened entry folded and its neighbour unfolded when an AI saved.
   */
  let open = $state<Record<string, boolean>>({});
  /**
   * An entry's key: its time, and for entries that share a millisecond
   * (two writers, a clock step, ERRU-009) how many older ones do. Stable
   * when new entries arrive on top.
   */
  const keys = $derived(activity.map((a, i) => `${a.ts_ms}:${activity.slice(i + 1).filter((b) => b.ts_ms === a.ts_ms).length}`));
  /**
   * The entry "Revert" undoes, the newest one a revert would take, by the
   * core's own walk (store.rs `latest_undone`): not undone by a later
   * revert (`undone`), not a revert itself, not the user's "offer figures
   * put back". The button is drawn on it when an AI made it and names it to
   * the core (ERR-801); an AI's revert entry gets none (UX-1505, UI-1501),
   * nor the entry that created the job (`created`, HUNT-1601).
   */
  const latest = $derived(activity.findIndex((a) => !a.undone && entryKind(a) !== "revert" && !(a.restores != null && byAUser(a.actor))));
  let list: HTMLOListElement | undefined = $state();
  /**
   * Runs a history button's action. Its button is gone once the action is
   * done (the entry it was drawn on changed): from the keyboard the focus
   * then stays on the history list (UI-902).
   */
  const press = (run: () => void | Promise<void>) => pressKeepingFocus(run, () => list);
  $effect(() => {
    if (shown == null) return;
    const i = activity.findIndex((a) => a.ts_ms === shown);
    shown = null;
    if (i < 0) return;
    open[keys[i]] = true;
    tick().then(() => list?.children[i]?.scrollIntoView({ block: "nearest" }));
  });
  /** What the "show changes" list holds: for the engine's recalculation each figure old -> new (UX-702a). */
  const rows = (a: ActivityItem) => (a.refreshed?.differences.length ? a.refreshed.differences : null);
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
  <ol bind:this={list} class="history" tabindex="-1" aria-label={t("ai.history")}>
    {#each activity as a, i (keys[i])}
      {@const ai = isAi(a.actor)}
      {@const figures = rows(a)}
      {@const k = keys[i]}
      <li class:ai class:undone={a.undone} data-testid="history-entry">
        <div class="head">
          <!-- Tectora's own entry is neither the user's (cursor) nor an AI's (sparkle). -->
          <span class="who" class:ai><Icon name={ai ? "sparkle" : byAUser(a.actor) ? "cursor" : "refresh"} size={13} />{actorLabel(a.actor)}</span>
          {#if a.undone}<span class="undone-mark" data-testid="history-undone">{t("ai.undone")}</span>{/if}
          <time>{relativeTime(a.ts_ms)}</time>
        </div>
        <p class="summary">{historyText(a, activity, recalculation?.entry_ts_ms)}</p>
        {#if figures}
          <button class="link" aria-expanded={!!open[k]} onclick={() => (open[k] = !open[k])}>
            {open[k] ? t("ai.hideChanges") : t("ai.showChanges", { n: figures.length })}
          </button>
          {#if open[k]}
            <ul data-testid="refreshed-figures">
              {#each figures as d (d.key)}
                <li>{describeFigure(d, project)}</li>
              {/each}
            </ul>
          {/if}
        {:else if a.changes.length}
          <button class="link" onclick={() => (open[k] = !open[k])}>
            {open[k] ? t("ai.hideChanges") : t("ai.showChanges", { n: a.changes.length })}
          </button>
          {#if open[k]}
            <ul>
              {#each a.changes as c}
                <li onpointerenter={() => onfocus(c)} onpointerleave={() => onfocus(null)}>
                  {describeChange(c, project)}
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
        <!-- Not on the entry that created the job: there is no version before it (HUNT-1601). -->
        {#if i === latest && ai && !a.created}
          <button class="btn sm" aria-disabled={busy} aria-busy={busy} onclick={() => !busy && press(() => onrevert(a.ts_ms))}><Icon name="undo" size={14} />{t("ai.revert")}</button>
        {:else if a.actor === ENGINE_ACTOR && recalculation?.entry_ts_ms === a.ts_ms}
          <!-- The offer's own figures back (owner decision 2026-11-22, UX-702d), on the recalculation the
               stored result still is, wherever it is in the history by now (UX-802): by this entry, and
               only the stored result (ERR-801). -->
          <button class="btn sm" aria-disabled={busy} aria-busy={busy} onclick={() => !busy && press(() => onrestore(a.ts_ms))}><Icon name="undo" size={14} />{t("snap.revertRefresh")}</button>
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
  /* Undone by a later revert: muted, with a small mark (UX-1505). */
  li.undone .summary {
    color: var(--text-2);
  }
  .undone-mark {
    margin-left: auto;
    margin-right: 8px;
    color: var(--text-2);
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
  /* A label that wraps (German) makes the button taller, never runs over its border (UI-901). */
  .btn.sm {
    height: auto;
    min-height: 28px;
    padding: 4px 10px;
    font-size: 12.5px;
    line-height: 1.3;
    white-space: normal;
    text-align: left;
  }
  .history:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
</style>
