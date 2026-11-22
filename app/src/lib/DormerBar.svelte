<script lang="ts">
  // DACAD's dormer definition (Gaube definieren): type, width, position,
  // eave height and pitches of the selected dormer, docked like the wall bar.
  import Icon from "./Icon.svelte";
  import { freshBar } from "./dblclick";
  import { numberLocale, t } from "./i18n.svelte";
  import type { CalcResult, Dormer, Project } from "./types";
  import { materialIds, materialName } from "./materials";
  import { DORMER_KINDS, buildDormer, defaultParams, parseDormer, roofHeightAt, type DormerKind, type DormerParams } from "./dormers";
  import { MAX_ROUND_SEGMENTS } from "./roundroof";
  import { formatNumber, numeric } from "./numeric.svelte";
  import { hscroll } from "./hscroll";
  import { pageStep, stepIndex } from "./itemstep";
  import { prefs, savePrefs } from "./settings.svelte";
  import PlaceFields from "./PlaceFields.svelte";
  import { placementOf } from "./items";
  import { api } from "./api";
  import { dormerRefusal, toAppError } from "./errors";
  import { notify, notifyError } from "./notify.svelte";
  import type { BarEdit } from "./jobedits";
  import { PITCH } from "./limits";

  let {
    project,
    result,
    index = $bindable(),
    onchange,
    ondelete,
    onedit,
    onplace,
    edit,
  }: {
    project: Project;
    result: CalcResult | null;
    index: number | null;
    /** Stores `d` as dormer `at` (the one the value was typed for, even when the selection moved since). */
    onchange: (d: Dormer, at: number) => void;
    ondelete: () => void;
    /** Opens the dormer window (plan and section). */
    onedit?: () => void;
    /** A distance on the roof face typed (from eave, left, right; the others null). */
    onplace?: (fromEave: number | null, fromLeft: number | null, fromRight: number | null) => unknown;
    /** Queues a core-backed edit of item `index` of `list`, held now (the editor's `barEdit`, jobedits.ts). */
    edit: BarEdit;
  } = $props();
  const placement = $derived(index !== null ? placementOf(result, { kind: "dormer", index }) : null);

  const dormer = $derived(index !== null ? project.dormers[index] : null);
  const params = $derived(dormer ? parseDormer(dormer) : null);
  /** Host roof height at the middle of the front of a dormer, for "on the roof" eaves. */
  function hostOf(p: DormerParams | null): number | null {
    if (!p) return null;
    const m = {
      x: p.start.x + (Math.cos(p.angle) * p.width) / 2,
      y: p.start.y + (Math.sin(p.angle) * p.width) / 2,
    };
    // Just inside the front, where the host face is.
    const n = { x: -Math.sin(p.angle) * 0.01, y: Math.cos(p.angle) * 0.01 };
    return roofHeightAt(result, { x: m.x + n.x, y: m.y + n.y });
  }
  const host = $derived(hostOf(params));
  const kind = $derived<DormerKind | null>(
    params && params.kind === "gable" && host !== null && Math.abs(params.eave_height - host) < 0.01 ? "pointed" : (params?.kind ?? null),
  );
  const angleDeg = $derived(params ? +(((params.angle * 180) / Math.PI + 360) % 360).toFixed(2) : 0);


  /**
   * A field of the bar changed: checked by the core, then stored on the
   * dormer it was typed for (held at this moment, so ▶, PageDown or an AI
   * save that moves it in the list does not send the value elsewhere:
   * UI-101, ERR-302). Edits run one after another, each on the dormer as
   * the one before left it, so quick arrow steps and a second field typed
   * while a check is out all land (UI-201). Only the field typed is the
   * user's: the rest is the dormer as it is when the check starts.
   */
  const set = (patch: Partial<DormerParams>) => {
    if (index === null) return;
    return edit(
      "dormers",
      index,
      async (job, at) => {
        const d = patched(parseDormer($state.snapshot(job.dormers[at]) as Dormer), patch);
        if (!d) return null;
        // The core says whether the dormer can stand there (an eave below the
        // roof, a pitch under 2°), as for MCP add_dormer (MATH-001, MATH-003).
        const probe = { ...$state.snapshot(job), dormers: job.dormers.map((x, i) => (i === at ? d : $state.snapshot(x))) } as Project;
        await api.checkDormer(probe, at);
        return d;
      },
      (d, at) => {
        if (d) onchange(d, at);
      },
    ).catch((e) => {
      const why = dormerRefusal(e);
      if (why === null) {
        notifyError(e, "calc");
        return false;
      }
      notify({ kind: "error", code: toAppError(e).code, title: t("dormer.refusedTitle"), message: why, detail: toAppError(e).detail });
      return why;
    });
  };
  /** Dormer `cur` with `patch`: the kind's own defaults follow a kind change. */
  function patched(cur: DormerParams, patch: Partial<DormerParams>): Dormer | null {
    const next = { ...cur, ...patch };
    if (next.kind === "general") return null;
    // A front pitch carries over only between kinds that have one.
    const fronted = (k: DormerKind) => k === "shed" || k === "trapezoid";
    if (patch.kind && fronted(patch.kind) && !fronted(cur.kind)) {
      next.front_pitch_deg = defaultParams(patch.kind, next.start, next.angle, next.eave_height).front_pitch_deg;
    }
    const h = hostOf(cur);
    if (next.kind === "pointed" && h !== null && patch.kind) next.eave_height = +h.toFixed(3);
    return buildDormer(next);
  }

  /** Position, direction and material sit in the "More" row, opened per user (UX-008, UI-008). */
  const moreOpen = $derived(!!prefs.dormerMore);
  function toggleMore() {
    prefs.dormerMore = !prefs.dormerMore;
    savePrefs();
  }
  /** What the dormer has set in the closed "More" row, so nothing is hidden silently. */
  const moreSet = $derived(params && params.material !== 1 ? [`${t("edge.material")}: ${materialName(project, params.material)}`] : []);
  const moreTitle = $derived(moreSet.length ? `${t("dormer.moreHint")}\n${t("bar.moreSet", { list: moreSet.join(", ") })}` : t("dormer.moreHint"));

  function step(d: number) {
    const next = stepIndex(index, project.dormers.length, d);
    if (next !== null) index = next;
  }
</script>

<div class="bar" role="toolbar" aria-label={t("dormer.title")} use:pageStep={step} use:freshBar>
  <div class="row" use:hscroll>
    <div class="nav">
      <button class="icon-btn" onclick={() => step(-1)} title="{t('dormer.prev')} (PageUp)" aria-label={t("dormer.prev")}><Icon name="left" /></button>
      <div class="which">
        <strong>{t("dormer.n", { n: (index ?? 0) + 1 })}</strong>
        <span>{t("def.of", { n: project.dormers.length })}</span>
      </div>
      <button class="icon-btn" onclick={() => step(1)} title="{t('dormer.next')} (PageDown)" aria-label={t("dormer.next")}><Icon name="right" /></button>
    </div>

    {#if params && kind}
      {#if kind === "general"}
        <p class="tip">{t("dormer.general")}</p>
      {:else}
        <label>
          <span>{t("dormer.kind")}</span>
          <select value={kind} onchange={(e) => set({ kind: (e.target as HTMLSelectElement).value as DormerKind })}>
            {#each DORMER_KINDS as k}
              <option value={k}>{t(`dormer.${k}`)}</option>
            {/each}
          </select>
        </label>
        <label>
          <span>{t("dormer.width")}</span>
          <div class="unit"><input type="text" use:numeric={{ value: params.width, step: 0.05, min: 0.1, onchange: (v) => set({ width: v }) }} /><em>m</em></div>
        </label>
        {#if kind === "round"}
          <label>
            <span>{t("round.segments")}</span>
            <input type="text" use:numeric={{ value: params.segments, step: 1, min: 0, max: MAX_ROUND_SEGMENTS, onchange: (v) => set({ segments: Math.round(v) }) }} />
          </label>
        {/if}
        {#if kind !== "shed" && kind !== "round"}
          <label>
            <span>{kind === "trapezoid" ? t("dormer.cheekPitch") : t("edge.pitch")}</span>
            <div class="unit"><input type="text" use:numeric={{ value: params.pitch_deg, step: 1, min: PITCH.min, max: PITCH.max, onchange: (v) => set({ pitch_deg: v }) }} /><em>°</em></div>
          </label>
        {/if}
        {#if kind === "shed" || kind === "trapezoid"}
          <label>
            <span>{t("dormer.frontPitch")}</span>
            <div class="unit"><input type="text" use:numeric={{ value: params.front_pitch_deg, step: 1, min: PITCH.min, max: PITCH.max, onchange: (v) => set({ front_pitch_deg: v }) }} /><em>°</em></div>
          </label>
        {/if}
        {#if kind === "half_hip"}
          <label>
            <span>{t("dormer.halfHipHeight")}</span>
            <div class="unit"><input type="text" use:numeric={{ value: params.half_hip_height, step: 0.05, min: 0.05, onchange: (v) => set({ half_hip_height: v }) }} /><em>m</em></div>
          </label>
          <label>
            <span>{t("edge.halfHipPitch")}</span>
            <div class="unit"><input type="text" use:numeric={{ value: params.half_hip_pitch_deg, step: 1, min: PITCH.min, max: PITCH.max, onchange: (v) => set({ half_hip_pitch_deg: v }) }} /><em>°</em></div>
          </label>
        {/if}
      {/if}
      <div class="end">
        {#if onedit && kind !== "general"}
          <button class="btn sm edit" onclick={onedit} title={t("dormer.editHint")} aria-label={t("dormer.edit")}><Icon name="draw" size={15} /><span class="label">{t("dormer.edit")}</span></button>
        {/if}
        {#if kind !== "general"}
          <button class="btn sm more-btn" class:on={moreOpen} data-testid="dormer-more" aria-expanded={moreOpen} aria-controls="dormer-more-row"
            aria-describedby={moreSet.length ? "dormer-more-set" : undefined}
            title={moreTitle} onclick={toggleMore}>
            {t("bar.more")}
            {#if moreSet.length}<b class="count" data-testid="dormer-more-count">{moreSet.length}</b><span class="sr-only" id="dormer-more-set">{t("bar.moreSet", { list: moreSet.join(", ") })}</span>{/if}
          </button>
        {/if}
        <button class="btn sm ghost danger del" onclick={ondelete} title={t("dormer.delete")} aria-label={t("dormer.delete")}><Icon name="trash" size={15} /><span class="label">{t("dormer.delete")}</span></button>
      </div>
    {/if}
  </div>

  {#if params && kind && kind !== "general" && moreOpen}
    <div class="row more" id="dormer-more-row" use:hscroll>
      {#if onplace}<PlaceFields {placement} {onplace} />{/if}
      <label title={host !== null ? t("dormer.hostHint", { h: formatNumber(host, numberLocale(), 2) }) : undefined}>
        <span>{t("edge.eaveHeight")}</span>
        <div class="unit"><input type="text" disabled={kind === "pointed"} use:numeric={{ value: params.eave_height, step: 0.05, onchange: (v) => set({ eave_height: v }) }} /><em>m</em></div>
      </label>
      <label>
        <span>{t("dormer.frontOverhang")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.overhang, step: 0.05, min: 0, onchange: (v) => set({ overhang: v }) }} /><em>m</em></div>
      </label>
      <label>
        <span>{t("dormer.sideOverhang")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.side_overhang, step: 0.05, min: 0, onchange: (v) => set({ side_overhang: v }) }} /><em>m</em></div>
      </label>
      <label>
        <span>X</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.start.x, step: 0.05, onchange: (v) => set({ start: { ...params.start, x: v } }) }} /><em>m</em></div>
      </label>
      <label>
        <span>Y</span>
        <div class="unit"><input type="text" use:numeric={{ value: params.start.y, step: 0.05, onchange: (v) => set({ start: { ...params.start, y: v } }) }} /><em>m</em></div>
      </label>
      <label>
        <span>{t("edge.direction")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: angleDeg, step: 1, onchange: (v) => set({ angle: (v * Math.PI) / 180 }) }} /><em>°</em></div>
      </label>
      <label title={t("edge.materialHint")}>
        <span>{t("edge.material")}</span>
        <select value={params.material} onchange={(e) => set({ material: +(e.target as HTMLSelectElement).value })}>
          {#each materialIds(project) as m}
            <option value={m}>{materialName(project, m)}</option>
          {/each}
        </select>
      </label>
    </div>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px 10px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    min-height: 70px;
    min-width: 0;
  }
  /* One line each, scrolled sideways when narrow: at most two rows (UI-008). */
  .row {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    overflow-x: auto;
    padding-bottom: 2px;
  }
  .row > :global(*) {
    flex-shrink: 0;
  }
  .row.more {
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .row.more .unit input {
    width: 92px;
  }
  /* Stays in view at the row's end when a narrow window scrolls the row. */
  /* More and Delete stay in view at the row's end when a narrow window scrolls the row. */
  .end {
    position: sticky;
    right: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    margin-bottom: 1px;
    background: var(--surface);
    box-shadow: -8px 0 8px -4px var(--surface);
  }
  .more-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .more-btn.on {
    background: var(--accent-soft);
    border-color: var(--accent-soft);
  }
  .count {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11.5px;
    font-weight: 700;
    line-height: 18px;
    text-align: center;
  }
  .more-btn.on .count {
    background: var(--surface);
  }
  .btn.sm {
    height: 32px;
    padding: 0 10px;
    font-size: 13px;
    white-space: nowrap;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 10px;
    border-right: 1px solid var(--border);
    align-self: stretch;
  }
  .which {
    display: flex;
    flex-direction: column;
    min-width: 64px;
    line-height: 1.25;
  }
  .which strong {
    font-size: 14px;
  }
  .which span {
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
  }
  select {
    height: 34px;
    width: 140px;
  }
  /* Where the bar is narrow the delete keeps its tooltip and name but shows only its icon. */
  .bar {
    container-type: inline-size;
  }
  .edit {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  @container (max-width: 820px) {
    .del .label,
    .edit .label {
      display: none;
    }
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 84px;
    padding-right: 24px;
    padding-left: 8px;
  }
  .unit em {
    position: absolute;
    right: 9px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
  }
  .danger {
    color: var(--danger);
  }
  .tip {
    align-self: center;
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
</style>
