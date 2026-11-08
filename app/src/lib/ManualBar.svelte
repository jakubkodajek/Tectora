<script lang="ts">
  // The selected hand-drawn roof line or added face (DACAD: Dachlinienbox,
  // Neue Dachfläche), docked like the wall bar.
  import Icon from "./Icon.svelte";
  import { m2 } from "./format";
  import { t } from "./i18n.svelte";
  import { COUNTED, MANUAL_CODES, faceArea, lineLength, type ManualSel } from "./manual";
  import type { ManualFace, ManualLine, Project } from "./types";
  import { numeric } from "./numeric.svelte";

  let {
    project,
    sel = $bindable(),
    online,
    onface,
    ondelete,
  }: {
    project: Project;
    sel: ManualSel;
    online: (l: ManualLine) => void;
    onface: (f: ManualFace) => void;
    ondelete: () => void;
  } = $props();

  const list = $derived(sel ? (sel.kind === "line" ? (project.manual_lines ?? []) : (project.manual_faces ?? [])) : []);
  const line = $derived(sel?.kind === "line" ? (project.manual_lines ?? [])[sel.index] ?? null : null);
  const face = $derived(sel?.kind === "face" ? (project.manual_faces ?? [])[sel.index] ?? null : null);


  function step(d: number) {
    if (!sel || list.length === 0) return;
    sel = { kind: sel.kind, index: (sel.index + d + list.length) % list.length };
  }
</script>

<div class="bar" role="toolbar" aria-label={t(sel?.kind === "face" ? "manual.face" : "manual.line")}>
  {#if sel}
    <div class="nav">
      <button class="icon-btn" onclick={() => step(-1)} aria-label={t("opening.prev")}><Icon name="left" /></button>
      <div class="which">
        <strong>{t(sel.kind === "face" ? "manual.faceN" : "manual.lineN", { n: sel.index + 1 })}</strong>
        <span>{t("def.of", { n: list.length })}</span>
      </div>
      <button class="icon-btn" onclick={() => step(1)} aria-label={t("opening.next")}><Icon name="right" /></button>
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
        <div class="unit"><input type="text" use:numeric={{ value: lineLength(line), step: 0.01, min: 0,
            onchange: (v) => online({ ...$state.snapshot(line), length: v }),
            onclear: () => online({ ...$state.snapshot(line), length: null }) }} /><em>m</em></div>
      </label>
      {#if line.length != null}
        <button class="link" onclick={() => online({ ...$state.snapshot(line), length: null })}>{t("manual.measured")}</button>
      {/if}
      <label>
        <span>{t("manual.label")}</span>
        <input type="text" class="text" value={line.label ?? ""} onchange={(e) => online({ ...$state.snapshot(line), label: e.currentTarget.value })} />
      </label>
      <p class="tip">{t(COUNTED.has(line.code) ? "manual.counted" : "manual.notCounted")}</p>
    {:else if face}
      <label>
        <span>{t("edge.pitch")}</span>
        <div class="unit"><input type="text" use:numeric={{ value: face.pitch_deg, step: 0.5, min: 0, max: 89.9, onchange: (v) => onface({ ...$state.snapshot(face), pitch_deg: v }) }} /><em>°</em></div>
      </label>
      <label>
        <span>{t("manual.label")}</span>
        <input type="text" class="text" value={face.label ?? ""} onchange={(e) => onface({ ...$state.snapshot(face), label: e.currentTarget.value })} />
      </label>
      <p class="tip">{t("manual.faceArea", { a: m2(faceArea(face)), n: face.points.length })}</p>
    {/if}
    <div class="spacer"></div>
    <button class="icon-btn danger" onclick={ondelete} title={t("manual.delete")} aria-label={t("manual.delete")}><Icon name="trash" /></button>
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
