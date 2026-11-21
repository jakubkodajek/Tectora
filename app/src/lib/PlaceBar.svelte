<script lang="ts">
  // The bottom bar while a placing tool is active (dormer, chimney, roof
  // window, opening, roof line, manual face): the tool's name and what to
  // click, with the points picked so far and Cancel, like the draw bar
  // (review UX-012). Without it the wall bar pointed at walls and the
  // tool's own instructions were a cut-off hint line in the plan.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { placeSteps, type PlaceKind } from "./placesteps";

  let {
    kind,
    points = 0,
    onundo,
    onclose,
    oncancel,
  }: {
    kind: PlaceKind;
    /** Points of a roof line or face picked so far. */
    points?: number;
    onundo?: () => void;
    /** Closes the face being drawn (Enter does the same); only for a manual face. */
    onclose?: () => void;
    oncancel: () => void;
  } = $props();

  const steps = $derived(placeSteps(kind, points));
</script>

<div class="bar" role="toolbar" aria-label={t(steps.title)} data-testid="place-bar" data-kind={kind}>
  <div class="title">
    <strong>{t(steps.title)}</strong>
    {#if steps.counted}<span data-testid="place-points">{t("place.points", { n: points })}</span>{/if}
  </div>
  <p class="steps" data-testid="place-steps">{t(steps.text)}</p>
  <div class="actions">
    {#if steps.counted && onundo}
      <button class="btn sm" disabled={points === 0} onclick={onundo}><Icon name="undo" size={15} />{t("draw.undo")}</button>
    {/if}
    {#if onclose}
      <button class="btn sm" disabled={points < 3} onclick={onclose} title="{t('place.closeFace')} (Enter)"><Icon name="check" size={15} />{t("place.closeFace")}</button>
    {/if}
    <button class="btn sm" onclick={oncancel} title="{t('common.cancel')} (Esc)">{t("common.cancel")}</button>
  </div>
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    padding: 10px 14px;
    background: color-mix(in srgb, var(--accent) 5%, var(--surface));
    border-top: 2px solid var(--accent);
    min-height: 56px;
  }
  .title {
    display: flex;
    flex-direction: column;
    min-width: 110px;
    line-height: 1.3;
  }
  .title span {
    font-size: 12px;
    color: var(--text-2);
  }
  .steps {
    flex: 1 1 260px;
    margin: 0;
    font-size: 13px;
    line-height: 1.4;
    color: var(--text);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
  .btn.sm {
    height: 32px;
    padding: 0 12px;
    font-size: 13px;
    white-space: nowrap;
  }
</style>
