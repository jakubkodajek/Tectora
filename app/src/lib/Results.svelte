<script lang="ts">
  // Results in DACAD's terms: roof areas per part and line lengths by
  // DACAD line code (T eave, F ridge, G hip, K valley, O verge, P shed top edge, c pitch break).
  import { figureName, formatFigure } from "./changes";
  import { hasBuilding } from "./offer";
  import { m, m2, num } from "./format";
  import { numberLocale, t } from "./i18n.svelte";
  import { materialName } from "./materials";
  import { areaRows, failedParts, isFacesOnly, joinedWith, lineRowsShown, missingResult, roofState, warningText } from "./results";
  import type { Described } from "./errors";
  import { inputWarnings } from "./plausible";
  import { legend, lineName, SUB_CODES } from "./linecodes";
  import { partLabel } from "./partname";
  import { api } from "./api";
  import type { CalcBreakdown, CalcResult, Project, ResultRefresh } from "./types";

  let {
    result,
    project = null,
    onstore,
    storing = false,
    blocked = null,
    failure = null,
    pending = false,
    recalculated = null,
    onrestore,
    restoring = false,
  }: {
    result: CalcResult | null;
    project?: Project | null;
    onstore?: () => void;
    /** The result is being stored: the button is off. */
    storing?: boolean;
    /** Why the figures cannot become the offer's stored result now (the core gave no roof), or null. */
    blocked?: string | null;
    /** Why the last calculation of the whole job failed (too complex, …), kept until one succeeds (UX-101). */
    failure?: Described | null;
    /** A recalculation is on its way: the figures are of the roof before the last edit (ERR-204). */
    pending?: boolean;
    /** The engine recalculated the stored result (when: `at`, its entry: `entry_ts_ms`) and nothing stored one since (UX-702b). */
    recalculated?: (ResultRefresh & { at: number; entry_ts_ms?: number }) | null;
    /** Puts the offer's own figures back in place of the recalculation logged at `ts` (HUNT-902: also once the history no longer lists the entry). */
    onrestore?: (ts: number) => void;
    /** A put-back or another review action is running: the button is off. */
    restoring?: boolean;
  } = $props();
  const day = (ms: number) => new Date(ms).toLocaleDateString(numberLocale());

  // What is on the roof (UX-005): counts and window sizes from the job,
  // the area each kind took off the roof from the core's breakdown.
  const objects = $derived(
    project
      ? {
          windows: project.windows ?? [],
          chimneys: project.chimneys?.length ?? 0,
          holes: project.roof_openings?.length ?? 0,
          dormers: project.dormers?.length ?? 0,
        }
      : null,
  );
  const hasObjects = $derived(!!objects && objects.windows.length + objects.chimneys + objects.holes + objects.dormers > 0);
  let breakdown = $state<CalcBreakdown | null>(null);
  let seq = 0;
  $effect(() => {
    void result;
    if (!hasObjects || !project) {
      breakdown = null;
      return;
    }
    const snapshot = $state.snapshot(project) as Project;
    const mine = ++seq;
    api
      .calcBreakdown(snapshot)
      .then((b) => mine === seq && (breakdown = b))
      .catch(() => mine === seq && (breakdown = null));
  });
  /** The core's deduction per object kind, summed over the buildings for display. */
  const deducted = $derived.by(() => {
    const by: Record<string, number> = {};
    for (const part of breakdown?.parts ?? []) for (const o of part.openings ?? []) by[o.kind] = (by[o.kind] ?? 0) + o.area;
    return by;
  });
  const deductedTotal = $derived(Object.values(deducted).reduce((a, b) => a + b, 0));

  /** The stored result (figures of the offer) against the current one. */
  const stored = $derived(result?.stored_result ?? null);
  /**
   * A job past the draft without a stored result (dropped for an impossible
   * date, or an old job): the card says so and offers to store one, which
   * the load warning asks for (audit 2026-11-23 UX-902).
   */
  const noneStored = $derived(!stored && !!result && !!project && project.job.status !== "draft" && !project.result_snapshot);
  const diffLabel = (key: string) => figureName(key, project);
  /** A roof of hand-drawn faces only: no "ridge 0,00 m" and no rows of zeros (UX-302). */
  const facesOnly = $derived(isFacesOnly(project));
  /** Why the current figures cannot be stored: the core gave no roof, or there is no building at all (UI-206). */
  const storeBlocked = $derived(blocked ?? (project && !hasBuilding(project) ? t("res.empty") : null));
  const fmtDiff = formatFigure;
  /** Per part, the parts the calculation joined its roof with (named under the part's area). */
  const joins = $derived(joinedWith(result));
  /** Parts and hand-drawn faces with their areas: everything the total counts (UX-009). */
  const areas = $derived(areaRows(result));

  /** Area per material, shown once the roof has more than one material or a non-default one. */
  const materials = $derived(
    (result?.totals.area_by_material ?? []).filter(([, a]) => Math.abs(a) >= 0.005),
  );

  const totals = $derived(result?.totals ?? null);
  /** The core gave no roof at all: the zeros it returns are no figures (ERR-004). */
  const failed = $derived(roofState(result) === "none");
  /** Buildings without a roof while the others have theirs: only their rows are "—" (ERRU-003). */
  const failedSet = $derived(roofState(result) === "partial" ? failedParts(result) : []);
  const failedNames = $derived(failedSet.map((i) => partLabel(result!.parts[i].name, i)).join(", "));
  const NONE = "—";
  /** No building yet (empty job, footprint being drawn): a dash and a hint, not 0,00 (UX-011). */
  const missing = $derived(missingResult(project, result, failure));
  const empty = $derived(missing === "empty");
  /** No figures to show at all. */
  const none = $derived(failed || missing !== null);

  /** Plausibility warnings of every part, with the part's name when there are several. */
  const warnings = $derived(
    (result?.parts ?? []).flatMap((p, i) =>
      (p.geometry.warnings ?? []).map((w) => ({ part: result!.parts.length > 1 ? partLabel(p.name, i) : null, text: warningText(w) })),
    ).concat(
      // Typed values that are allowed but unlikely (a lost decimal comma,
      // UX-009) and half-hips or breaks above the ridge (UX-003).
      inputWarnings(project, result),
    ),
  );
  const rows = $derived(
    totals
      ? [
          { code: "T", color: "var(--line-eave)", label: t("res.eave"), value: totals.eave_length, n: null },
          { code: "F", color: "var(--line-ridge)", label: t("res.ridge"), value: totals.ridge_length, n: null },
          { code: "G", color: "var(--line-hip)", label: t("res.hipShort"), value: totals.hip_length, n: totals.hip_count },
          { code: "K", color: "var(--line-valley)", label: t("res.valleyShort"), value: totals.valley_length, n: totals.valley_count },
          { code: "O", color: "var(--line-verge)", label: t("res.verge"), value: totals.verge_length, n: null },
          { code: "P", color: "var(--line-pult)", label: t("res.pult"), value: totals.pult_length, n: null },
          { code: "c", color: "var(--line-knick)", label: t("res.knick"), value: totals.knick_length, n: null },
          { code: "S", color: "var(--line-gorge)", label: t("res.gorge"), value: totals.gorge_length ?? 0, n: null },
          { code: "W", color: "var(--line-abutment)", label: t("res.abutment"), value: totals.abutment_length, n: null },
        ].filter((r) => r.value >= 0.005 || !["P", "c", "S", "W"].includes(r.code))
      : [],
  );
  const shownRows = $derived(lineRowsShown(rows, facesOnly && !none));
  /** The core's lengths by code (OL, OR, WV, …), for the parts of the O and W rows and the legend. */
  const codes = $derived(result?.codes ?? {});
  /** "Napojení čelní 2,60 m · boční …": the W row split by kind, which matter for the flashing (UX-013). */
  const subRows = (code: string) =>
    (SUB_CODES[code] ?? []).filter((c) => (codes[c] ?? 0) >= 0.005).map((c) => ({ code: c, label: lineName(c), value: codes[c] }));
</script>

<div class="results" class:pending={pending && !!result} aria-busy={pending}>
  <div class="hero">
    <span>{t("res.area")}</span>
    <strong data-testid="res-area" title={failed ? t("res.notCalculated") : undefined}>{totals && !none ? m2(totals.roof_area) : NONE}</strong>
    {#if empty}<small data-testid="res-empty">{t("res.empty")}</small>
    {:else if missing === "failed" && failure}<small data-testid="res-calc-failed"><code>{failure.code}</code> {failure.message}</small>
    {:else if missing === "calculating"}<small data-testid="res-calculating">{t("res.calculating")}</small>
    {:else if failed}<small data-testid="res-not-calculated">{t("res.notCalculated")}</small>
    {:else if totals && !facesOnly}<small data-testid="res-ridge">{t("res.ridgeHeight", { v: num(totals.ridge_height) })}</small>{/if}
    {#if pending && result}<small class="stale" role="status" data-testid="res-stale">{t("res.calculating")}</small>{/if}
    {#if failedSet.length}<small data-testid="res-partial">{t("res.partialNote", { names: failedNames })}</small>{/if}
    {#if totals && !failed && deductedTotal >= 0.005}<small data-testid="res-deducted">{t("res.obj.netOf", { a: m2(deductedTotal) })}</small>{/if}
  </div>

  {#if warnings.length}
    <div class="warn" role="status">
      <div class="warn-head"><i></i>{t("res.warn.title")}</div>
      {#each warnings as w}
        <!-- The space is an expression: Svelte trims plain whitespace at the end of a block. -->
        <p>{#if w.part}<code>{w.part}</code>{" "}{/if}{w.text}</p>
      {/each}
    </div>
  {/if}

  {#if result && areas.length > 1}
    <h4>{t("res.areas")}</h4>
    <table class="areas">
      <tbody>
        {#each areas as a}
          <tr>
            <td>
              {a.name}
              {#if a.manual}
                <small class="joined">{t("bd.manual")}</small>
              {:else if joins[a.part]?.length}
                <small class="joined">{t("res.joinedTo", { names: joins[a.part].map((j) => (result.parts[j] ? partLabel(result.parts[j].name, j) : "")).join(", ") })}</small>
              {/if}
            </td>
            <td class="v">{failed || (!a.manual && failedSet.includes(a.part)) ? NONE : m2(a.area)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if materials.length > 1 || (materials.length === 1 && materials[0][0] !== 1)}
    <h4>{t("res.byMaterial")}</h4>
    <table>
      <tbody>
        {#each materials as [mat, a]}
          <tr><td>{materialName(project, mat)}</td><td class="v">{failed ? NONE : m2(a)}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if hasObjects && objects}
    <h4>{t("res.objects")}</h4>
    <table class="objects" data-testid="res-objects">
      <tbody>
        {#if objects.windows.length}
          <tr>
            <td>
              {t("res.obj.windows", { n: objects.windows.length })}
              <small class="joined">{objects.windows.map((w) => `${num(w.width)} × ${num(w.height)} m`).join(", ")}</small>
            </td>
            <td class="v">{#if deducted.window}−{m2(deducted.window)}{/if}</td>
          </tr>
        {/if}
        {#if objects.chimneys}
          <tr><td>{t("res.obj.chimneys", { n: objects.chimneys })}</td><td class="v">{#if deducted.chimney}−{m2(deducted.chimney)}{/if}</td></tr>
        {/if}
        {#if objects.holes}
          <tr><td>{t("res.obj.holes", { n: objects.holes })}</td><td class="v">{#if deducted.hole}−{m2(deducted.hole)}{/if}</td></tr>
        {/if}
        {#if objects.dormers}
          <tr><td>{t("res.obj.dormers", { n: objects.dormers })}</td><td class="v">{#if deducted.dormer}−{m2(deducted.dormer)}{/if}</td></tr>
        {/if}
      </tbody>
    </table>
    <p class="note">{t("res.obj.note")}</p>
  {/if}

  <h4>{t("res.lines")}</h4>
  <table data-testid="res-lines">
    <tbody>
      {#each shownRows as r}
        <tr class:zero={none || r.value < 0.005}>
          <td class="code"><i style="background:{r.color}"></i>{r.code}</td>
          <td>{r.label}{#if r.n && !failed}<span class="n">×{r.n}</span>{/if}
            {#if r.code === "W" && !failed && subRows("W").length}
              <small class="joined" data-testid="res-abutments">{subRows("W").map((s) => `${s.code} ${s.label} ${m(s.value)}`).join(" · ")}</small>
            {/if}</td>
          <td class="v">{none ? NONE : m(r.value)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="note" data-testid="res-legend">{legend(shownRows.map((r) => r.code), codes)}</p>

  {#if stored}
    <h4>{t("snap.title")}</h4>
    <!-- Figures that leave out a building the calculation cannot take are no
         comparison: equal figures would read "unchanged" (UX-103). -->
    <div class="snap" class:changed={stored.differences.length > 0 || !!blocked}>
      <div class="snap-head" data-testid="snap-state">
        <i class="sq"></i>
        <span title={blocked ?? undefined}>
          {#if recalculated && !blocked && !stored.differences.length}
            <!-- Not "unchanged since stored" with the date of the open: the engine stored these figures (UX-702b). -->
            {t("snap.recalculated", { date: day(recalculated.at), from: recalculated.from, to: recalculated.to })}
          {:else}
            {blocked ? t("snap.unchecked") : stored.differences.length ? t("snap.changed") : t("snap.same")}
            · {day(stored.stored.taken_ms)}
          {/if}
        </span>
      </div>
      {#if recalculated}
        {#if blocked || stored.differences.length}
          <p class="mono" data-testid="snap-recalculated">{t("snap.recalculated", { date: day(recalculated.at), from: recalculated.from, to: recalculated.to })}</p>
        {/if}
        <p class="mono" data-testid="snap-offer">{t("snap.offerStored", { date: day(recalculated.offer_ms) })}</p>
        {#if onrestore && recalculated.entry_ts_ms != null}
          {@const ts = recalculated.entry_ts_ms}
          <!-- The same put-back as on the history's entry, which the change log may have dropped by now (HUNT-902). -->
          <button class="btn sm" data-testid="snap-restore" aria-disabled={restoring} aria-busy={restoring} onclick={() => !restoring && onrestore?.(ts)}>{t("snap.revertRefresh")}</button>
        {/if}
      {/if}
      {#if stored.differences.length}
        <table class="diff">
          <!-- Which column is the offer and which the roof now (UX-205). -->
          <thead><tr><th></th><th class="v">{t("snap.colStored")}</th><th class="v">{t("snap.colNow")}</th></tr></thead>
          <tbody>
            {#each stored.differences as d}
              <tr><td>{diffLabel(d.key)}</td><td class="v">{fmtDiff(d.key, d.stored)}</td><td class="v">{fmtDiff(d.key, d.current)}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if stored.engine_changed}
        <p class="mono">{t("snap.engine", { v: stored.stored.engine_version })}</p>
      {/if}
      {#if stored.differences.length && onstore}
        <button class="btn sm" disabled={!!storeBlocked} aria-disabled={storing} aria-busy={storing} title={storeBlocked ?? undefined} onclick={() => !storing && onstore?.()}>{t("snap.store")}</button>
      {/if}
    </div>
  {:else if noneStored}
    <h4>{t("snap.title")}</h4>
    <div class="snap" data-testid="snap-none">
      <div class="snap-head" data-testid="snap-state">
        <i class="sq"></i>
        <span>{t("snap.none")}</span>
      </div>
      {#if onstore}
        <button class="btn sm" data-testid="snap-store" disabled={!!storeBlocked} aria-disabled={storing} aria-busy={storing} title={storeBlocked ?? undefined} onclick={() => !storing && onstore?.()}>{t("snap.store")}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* Figures of the roof before the last edit while its recalculation runs (ERR-204). */
  /* --text-2: --text-3 on the hero's --accent-soft is 2.67:1 (UI-305; the pair is in contrast.py). */
  .results.pending .hero strong {
    color: var(--text-2);
  }
  .results.pending .hero ~ * {
    opacity: 0.6;
  }
  .joined {
    display: block;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .results {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hero {
    display: flex;
    flex-direction: column;
    padding: 14px;
    border-radius: 10px;
    background: var(--accent-soft);
  }
  .hero span,
  .hero small {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .hero strong {
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  h4 {
    margin: 6px 0 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-3);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  td {
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  tr.zero {
    color: var(--text-3);
  }
  .code {
    width: 44px;
    font-weight: 700;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  }
  .code i {
    display: inline-block;
    width: 10px;
    height: 3px;
    margin-right: 8px;
    border-radius: 2px;
    vertical-align: middle;
  }
  .n {
    margin-left: 5px;
    color: var(--text-3);
    font-size: 12px;
  }
  .v {
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
    /* The figure and its unit stay on one line; the label column wraps instead (UI-006). */
    white-space: nowrap;
    padding-left: 8px;
  }
  .warn {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 12.5px;
  }
  .warn-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
  }
  .warn-head i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--ai);
  }
  .warn p {
    margin: 6px 0 0 16px;
    color: var(--text-2);
  }
  .warn code {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
  }
  .snap {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    font-size: 12.5px;
  }
  .snap-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sq {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--text-3);
  }
  .snap.changed .sq {
    background: var(--danger);
  }
  .diff th {
    padding: 0 0 2px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-align: left;
  }
  .diff th.v {
    text-align: right;
  }
  .diff td {
    padding: 4px 0;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 12px;
  }
  /* The two figure columns keep apart ("152,73 m²  142,29 m²", UX-205). */
  .diff td.v,
  .diff th.v {
    padding-left: 12px;
  }
  .mono {
    margin: 0;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .btn.sm {
    align-self: flex-start;
  }
  .note {
    margin: 0;
    font-size: 11.5px;
    color: var(--text-3);
  }
</style>
