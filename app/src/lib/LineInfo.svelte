<script lang="ts">
  // What a roof line is and how high it lies: shown while the pointer is on
  // a line in the plan or in 3D, and kept (pinned) after a click on it.
  import type { LineDetail, Project } from "./types";
  import { numberLocale, t } from "./i18n.svelte";
  import { LEVEL_DEG, LINE_TOKENS, lineTitle } from "./lineinfo";
  import { materialName, materialToken } from "./materials";
  import { wallNumber } from "./planlabels";
  import Icon from "./Icon.svelte";

  let {
    detail,
    project,
    pinned = false,
    onclose,
  }: {
    detail: LineDetail;
    project: Project;
    pinned?: boolean;
    onclose?: () => void;
  } = $props();

  const fmt = (v: number, d = 2) =>
    new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: d, maximumFractionDigits: d, useGrouping: false }).format(v);
  const deg = (v: number) => `${new Intl.NumberFormat(numberLocale(), { maximumFractionDigits: 2 }).format(v)}°`;
  const level = $derived(detail.slope_deg < LEVEL_DEG);
  const parts = $derived(project.parts.length);
</script>

<div class="line-info" class:pinned role="status" aria-label={t("lineinfo.title")} data-testid="line-info">
  <div class="head">
    <i class="sw" style:background={`var(${LINE_TOKENS[detail.kind] ?? "--text-2"})`}></i>
    <strong>{lineTitle(detail.code)}</strong>
    <code>{detail.code}</code>
    {#if pinned}
      <button class="x icon-btn" aria-label={t("lineinfo.close")} title={t("lineinfo.close")} onclick={() => onclose?.()}>
        <Icon name="x" size={14} />
      </button>
    {/if}
  </div>
  <dl>
    <dt>{t("lineinfo.length")}</dt>
    <dd data-testid="line-info-length">
      {fmt(detail.length)} m
      {#if Math.abs(detail.length - detail.plan_length) >= 0.005}<small>{t("lineinfo.planLength", { v: fmt(detail.plan_length) })}</small>{/if}
    </dd>
    {#if level}
      <dt>{t("lineinfo.height")}</dt>
      <dd data-testid="line-info-height">{fmt(detail.z_high)} m</dd>
    {:else}
      <dt>{t("lineinfo.heights")}</dt>
      <dd data-testid="line-info-height">{fmt(detail.z_low)} – {fmt(detail.z_high)} m</dd>
      <dt>{t("lineinfo.slope")}</dt>
      <dd>{deg(detail.slope_deg)}</dd>
    {/if}
    {#if detail.faces.length}
      <dt>{t("lineinfo.faces")}</dt>
      <dd class="faces">
        {#each detail.faces as f}
          <span>
            <i class="sw mat" style:background={`var(${materialToken(f.material)})`}></i>
            {f.wall === undefined ? t("lineinfo.dormer") : t("lineinfo.wall", { n: wallNumber(f.part, f.wall, parts) })}
            · {deg(f.pitch_deg)} · {materialName(project, f.material)}
          </span>
        {/each}
      </dd>
    {/if}
  </dl>
  {#if !pinned}<p class="li-hint">{t("lineinfo.pinHint")}</p>{/if}
</div>

<style>
  .line-info {
    position: absolute;
    z-index: 3;
    left: 10px;
    top: 36px;
    width: max-content;
    min-width: 190px;
    max-width: min(300px, calc(100% - 20px));
    padding: 7px 9px 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in srgb, var(--surface) 94%, transparent);
    box-shadow: var(--shadow);
    font-size: 12px;
    color: var(--text);
    pointer-events: none;
  }
  .line-info.pinned {
    pointer-events: auto;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 5px;
  }
  .head strong {
    font-weight: 600;
  }
  code {
    font: 11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    color: var(--text-2);
  }
  .sw {
    display: inline-block;
    flex: none;
    width: 14px;
    height: 3px;
    border-radius: 2px;
  }
  .sw.mat {
    width: 9px;
    height: 9px;
    border: 1px solid var(--border);
    vertical-align: -1px;
  }
  .x {
    margin-left: auto;
    width: 22px;
    height: 22px;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    margin: 0;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  dd small {
    margin-left: 4px;
    color: var(--text-2);
  }
  .faces {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .li-hint {
    margin: 5px 0 0;
    font-size: 11px;
    color: var(--text-2);
  }
</style>
