<script lang="ts">
  // Where the selected dormer, chimney, roof window or opening sits on its
  // roof face: from the eave along the slope and from both side edges. The
  // values come from the core (edit::position) and typing one moves the
  // object there (Tauri place_item, MCP place_item).
  import { t } from "./i18n.svelte";
  import { numeric } from "./numeric.svelte";
  import type { Placement } from "./types";

  let {
    placement,
    onplace,
    width = 108,
  }: {
    placement: Placement | null;
    /** One distance typed: the others are null (kept). */
    onplace: (fromEave: number | null, fromLeft: number | null, fromRight: number | null) => unknown;
    /** Field width, px. */
    width?: number;
  } = $props();
</script>

{#if placement}
  <div class="place" role="group" aria-label={t("place.title")} data-testid="place-fields" style="--fw: {width}px">
    <label title={t("place.fromEaveHint")}>
      <span>{t("place.fromEave")}</span>
      <div class="unit"><input type="text" data-testid="place-eave" use:numeric={{ value: placement.from_eave, digits: 3, step: 0.05, min: 0, max: 500, onchange: (v) => onplace(v, null, null) }} /><em>m</em></div>
    </label>
    <label title={t("place.fromLeftHint")}>
      <span>{t("place.fromLeft")}</span>
      <div class="unit"><input type="text" data-testid="place-left" use:numeric={{ value: placement.from_left, digits: 3, step: 0.05, min: 0, max: 500, onchange: (v) => onplace(null, v, null) }} /><em>m</em></div>
    </label>
    <label title={t("place.fromRightHint")}>
      <span>{t("place.fromRight")}</span>
      <div class="unit"><input type="text" data-testid="place-right" use:numeric={{ value: placement.from_right, digits: 3, step: 0.05, min: 0, max: 500, onchange: (v) => onplace(null, null, v) }} /><em>m</em></div>
    </label>
  </div>
{/if}

<style>
  .place {
    display: flex;
    align-items: flex-end;
    gap: 10px 14px;
    flex-wrap: wrap;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: var(--fw);
    padding-right: 24px;
    font-variant-numeric: tabular-nums;
  }
  .unit em {
    position: absolute;
    right: 9px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
    pointer-events: none;
  }
</style>
