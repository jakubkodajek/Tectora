<script lang="ts">
  // The fields of a curved-wall edit: a wall bent into an arc (bulge, + out)
  // or a corner rounded (radius), with the arc's wall count (empty: the
  // core picks it). The core makes the arc (`arc_edit`, the MCP tool
  // edit_walls' bend_wall / round_corner) and says when it does not fit.
  import { num } from "./format";
  import { t } from "./i18n.svelte";
  import { numeric } from "./numeric.svelte";

  let {
    mode,
    value = $bindable(),
    segments = $bindable(null),
    max = null,
    refused = null,
  }: {
    mode: "bend" | "round";
    /** The bulge (bend) or the radius (round), m. */
    value: number;
    segments?: number | null;
    /** The largest bulge, half the wall, m (bend only). */
    max?: number | null;
    /** The core's reason for not making the arc, in the user's language. */
    refused?: string | null;
  } = $props();

  /** At most this many walls in an arc (core `edit::arcs::MAX_ARC_SEGMENTS`). */
  const MAX_SEGMENTS = 64;
</script>

<div class="form">
  <label>
    <span>{mode === "bend" ? t("arc.bulge") : t("arc.radius")}</span>
    <div class="unit">
      <input type="text" data-testid="arc-value"
        use:numeric={{ value, step: 0.05, min: mode === "bend" ? -(max ?? 1e5) : 0.01, max: mode === "bend" ? (max ?? 1e5) : 1e5, live: true, onchange: (v) => (value = v) }} /><em>m</em>
    </div>
  </label>
  <p class="hint">{mode === "bend" ? t("arc.bulgeHint", { max: num(max ?? 0) }) : t("arc.roundHint")}</p>
  <label>
    <span>{t("arc.segments")}</span>
    <input type="text" placeholder={t("arc.segmentsAuto")}
      use:numeric={{ value: segments, step: 1, min: 2, max: MAX_SEGMENTS, live: true, onchange: (v) => (segments = Math.round(v)), onclear: () => (segments = null) }} />
  </label>
  {#if refused}
    <p class="note" role="alert" data-testid="arc-refused"><i></i><span>{refused}</span></p>
  {/if}
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: min(360px, 100%);
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
  }
  input {
    width: 100%;
  }
  .unit {
    position: relative;
  }
  .unit input {
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
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .note {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .note i {
    width: 8px;
    height: 8px;
    flex: none;
    background: var(--danger);
  }
</style>
