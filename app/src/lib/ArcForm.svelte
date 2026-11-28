<script lang="ts">
  // The fields of a curved-wall edit: a wall bent into an arc (bulge, + out)
  // or a corner rounded (radius), with the arc's wall count (empty: the
  // core picks it). The core makes the arc (`arc_edit`, the MCP tool
  // edit_walls' bend_wall / round_corner) and says when it does not fit.
  import { num } from "./format";
  import { t } from "./i18n.svelte";
  import { numeric } from "./numeric.svelte";
  import { SvelteMap } from "svelte/reactivity";

  let {
    mode,
    value = $bindable(),
    segments = $bindable(null),
    max = null,
    refused = $bindable(null),
    detail = $bindable(null),
    typing = $bindable(null),
    reshape = false,
  }: {
    mode: "bend" | "round";
    /** Bend: an arc changed (straightened and bent again on its chord, `reshape_arc`), `max` half its chord (UX-007). */
    reshape?: boolean;
    /** The bulge (bend) or the radius (round), m. */
    value: number;
    segments?: number | null;
    /** The largest bulge, half the wall, m (bend only). */
    max?: number | null;
    /** The core's reason for not making the arc, in the user's language; cleared once a field is typed again (UI-007). */
    refused?: string | null;
    /** The core's code and text under `refused`, for a reason we have no text for (ERR-004). */
    detail?: string | null;
    /** A field holds text it refuses: OK stops with this reason (ERR-003). */
    typing?: string | null;
  } = $props();

  /** Fields whose typed text cannot be used: their label and the field's reason. */
  const invalid = new SvelteMap<string, string>();
  const validity = (id: string, label: string) => (why: string | null) => {
    if (why === null) invalid.delete(id);
    else invalid.set(id, `${label}: ${why}`);
  };
  $effect(() => {
    const first = [...invalid.values()][0];
    typing = first ? t("num.fixValue", { why: first }) : null;
  });
  /** Another value typed: an earlier refusal is not about it any more. */
  const edited = () => {
    refused = null;
    detail = null;
  };

  /** At most this many walls in an arc (core `edit::arcs::MAX_ARC_SEGMENTS`). */
  const MAX_SEGMENTS = 64;
</script>

<div class="form">
  <label>
    <span>{mode === "bend" ? t("arc.bulge") : t("arc.radius")}</span>
    <div class="unit">
      <input type="text" data-testid="arc-value"
        use:numeric={{ value, step: 0.05, min: mode === "bend" ? -(max ?? 1e5) : 0.01, max: mode === "bend" ? (max ?? 1e5) : 1e5, live: true, onchange: (v) => { edited(); value = v; }, onvalidity: validity("value", mode === "bend" ? t("arc.bulge") : t("arc.radius")) }} /><em>m</em>
    </div>
  </label>
  <p class="hint">{mode === "bend" ? t(reshape ? "arc.reshapeHint" : "arc.bulgeHint", { max: num(max ?? 0) }) : t("arc.roundHint")}</p>
  <label>
    <span>{t("arc.segments")}</span>
    <div class="unit">
      <input type="text" placeholder={t("arc.segmentsAuto")}
        use:numeric={{ value: segments, step: 1, min: 2, max: MAX_SEGMENTS, live: true, onchange: (v) => { edited(); segments = Math.round(v); }, onclear: () => { edited(); segments = null; }, onvalidity: validity("segments", t("arc.segments")) }} />
    </div>
  </label>
  {#if refused}
    <p class="note" role="alert" data-testid="arc-refused"><i></i><span>{refused}{#if detail}<code data-testid="arc-refused-detail">{detail}</code>{/if}</span></p>
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
  .note code {
    display: block;
    margin-top: 2px;
    font: 11.5px/1.4 ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    color: var(--text-3);
    overflow-wrap: anywhere;
  }
  .note i {
    width: 8px;
    height: 8px;
    flex: none;
    background: var(--danger);
  }
</style>
