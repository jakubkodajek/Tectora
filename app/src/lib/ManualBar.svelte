<script lang="ts">
  // The selected hand-drawn roof line or added face (DACAD: Dachlinienbox,
  // Neue Dachfläche), docked like the wall bar.
  import Icon from "./Icon.svelte";
  import { m2 } from "./format";
  import { t } from "./i18n.svelte";
  import { COUNTED, MANUAL_CODES, lineLength, type ManualSel } from "./manual";
  import { api } from "./api";
  import { latest } from "./latest";
  import { notifyError } from "./notify.svelte";
  import { onDestroy } from "svelte";
  import type { ManualFace, ManualLine, Project } from "./types";
  import { numeric } from "./numeric.svelte";
  import { textfield } from "./textfield";
  import { pageStep, stepIndex } from "./itemstep";
  import { freshBar, pressTooSoon } from "./dblclick";

  let {
    project,
    sel = $bindable(),
    online,
    onface,
    ontopart,
    busy = false,
    ondelete,
  }: {
    project: Project;
    sel: ManualSel;
    online: (l: ManualLine) => void;
    onface: (f: ManualFace) => void;
    /** Replaces the selected face with a building part of the same outline. */
    ontopart?: () => void;
    /** "Make it a building" is under way: its button is off (UI-308). */
    busy?: boolean;
    ondelete: () => void;
  } = $props();
  // The bar replaces the face tool's bar right after "Close face", with
  // Delete where Close was: a double click on Close must not delete (UI-103).
  const shownAt = performance.now();

  const list = $derived(sel ? (sel.kind === "line" ? (project.manual_lines ?? []) : (project.manual_faces ?? [])) : []);
  const line = $derived(sel?.kind === "line" ? (project.manual_lines ?? [])[sel.index] ?? null : null);
  const face = $derived(sel?.kind === "face" ? (project.manual_faces ?? [])[sel.index] ?? null : null);
  /** The line or face the fields edit now: a field reused for the next one forgets the one before (UI-403). */
  const itemKey = $derived(sel ? `${sel.kind}:${sel.index}` : "");


  // The face's area comes from the core (`ManualFace::area`), asked again
  // after a pause while its corners or pitch change.
  let area = $state<number | null>(null);
  const askArea = latest(
    (f: ManualFace) => api.faceArea(f),
    (a) => (area = a),
    (e) => {
      area = null;
      notifyError(e, "calc");
    },
    60,
  );
  onDestroy(() => askArea.stop());
  $effect(() => {
    if (face) askArea.ask($state.snapshot(face) as ManualFace);
    else {
      askArea.stop();
      area = null;
    }
  });

  function step(d: number) {
    const next = sel ? stepIndex(sel.index, list.length, d) : null;
    if (sel && next !== null) sel = { kind: sel.kind, index: next };
  }
</script>

<div class="bar" role="toolbar" aria-label={t(sel?.kind === "face" ? "manual.face" : "manual.line")} use:pageStep={step} use:freshBar>
  {#if sel}
    <div class="nav">
      <button class="icon-btn" onclick={() => step(-1)} title="{t('opening.prev')} (PageUp)" aria-label={t("opening.prev")}><Icon name="left" /></button>
      <div class="which">
        <strong>{t(sel.kind === "face" ? "manual.faceN" : "manual.lineN", { n: sel.index + 1 })}</strong>
        <span>{t("def.of", { n: list.length })}</span>
      </div>
      <button class="icon-btn" onclick={() => step(1)} title="{t('opening.next')} (PageDown)" aria-label={t("opening.next")}><Icon name="right" /></button>
    </div>
    {#if line}
      <label>
        <span>{t("manual.type")}</span>
        <select value={line.code} onchange={(e) => online({ ...$state.snapshot(line), code: e.currentTarget.value })}>
          {#each MANUAL_CODES as c}<option value={c}>{t(`manual.code.${c}`)}</option>{/each}
        </select>
      </label>
      <label>
        <span>{t("manual.length")}</span>
        <div class="unit"><input type="text" use:numeric={{ key: itemKey, value: lineLength(line), step: 0.01, min: 0,
            onchange: (v) => online({ ...$state.snapshot(line), length: v }),
            onclear: () => online({ ...$state.snapshot(line), length: null }) }} /><em>m</em></div>
      </label>
      {#if line.length != null}
        <button class="link" onclick={() => online({ ...$state.snapshot(line), length: null })}>{t("manual.measured")}</button>
      {/if}
      <label>
        <span>{t("manual.label")}</span>
        <input type="text" class="text" data-testid="manual-label" use:textfield={{ value: line.label ?? "", onchange: (v) => online({ ...$state.snapshot(line), label: v }) }} />
      </label>
      <p class="tip">{t(COUNTED.has(line.code) ? "manual.counted" : "manual.notCounted")}</p>
    {:else if face}
      <label>
        <span>{t("edge.pitch")}</span>
        <div class="unit"><input type="text" use:numeric={{ key: itemKey, value: face.pitch_deg, step: 0.5, min: 0, max: 89.9, onchange: (v) => onface({ ...$state.snapshot(face), pitch_deg: v }) }} /><em>°</em></div>
      </label>
      <label>
        <span>{t("manual.label")}</span>
        <input type="text" class="text" data-testid="manual-label" use:textfield={{ value: face.label ?? "", onchange: (v) => onface({ ...$state.snapshot(face), label: v }) }} />
      </label>
      <p class="tip">{t("manual.faceArea", { a: area === null ? "…" : m2(area), n: face.points.length })}</p>
    {/if}
    <div class="spacer"></div>
    {#if face && ontopart}
      <button class="btn sm" aria-disabled={busy} aria-busy={busy} onclick={() => !busy && ontopart?.()} title={t("manual.toPartHint")}><Icon name="house" size={15} />{t("manual.toPart")}</button>
    {/if}
    {@const del = t(sel.kind === "line" ? "manual.deleteLine" : "manual.deleteFace")}
    <button class="btn sm ghost danger" onclick={(e) => !pressTooSoon(shownAt, e.detail) && ondelete()} title={del}><Icon name="trash" size={15} />{del}</button>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
    padding: 10px 14px 12px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    min-height: 70px;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 14px;
    border-right: 1px solid var(--border);
    align-self: stretch;
  }
  .which {
    display: flex;
    flex-direction: column;
    min-width: 92px;
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
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 92px;
    padding-right: 24px;
  }
  .unit em {
    position: absolute;
    right: 9px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
  }
  .text {
    width: 120px;
  }
  .link {
    align-self: center;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    padding: 0;
  }
  .spacer {
    flex: 1;
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
