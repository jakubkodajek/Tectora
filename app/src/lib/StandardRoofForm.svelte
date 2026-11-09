<script lang="ts">
  // DACAD's "Standard-Dach": pick a basic roof type for a rectangle and
  // enter its dimensions; shows a live plan preview with roof lines.
  import { api } from "./api";
  import { describeError, toAppError } from "./errors";
  import { notifyError } from "./notify.svelte";
  import PlanThumb from "./PlanThumb.svelte";
  import { m2 } from "./format";
  import { t } from "./i18n.svelte";
  import { numeric } from "./numeric.svelte";
  import { presets } from "./settings.svelte";
  import type { RoofPart, StandardKind } from "./types";
  import { STANDARD_KINDS, emptyProject } from "./types";
  import { regularPolygon } from "./transform";

  let { part = $bindable(null) }: { part: RoofPart | null } = $props();

  /** Longest side the dialog takes, m [no source: far beyond any building, inside the core's ±100 km]. */
  const MAX_SIDE = 10000;

  /** The standard roofs plus DACAD's regular polygon (Vieleck-Eingabe), all walls eaves. */
  type FormKind = StandardKind | "polygon";
  const KINDS: FormKind[] = [...STANDARD_KINDS, "polygon"];

  let kind = $state<FormKind>("gable");
  let sides = $state(8);
  let length = $state(12);
  let width = $state(8);
  let pitch = $state(presets.pitch_deg);
  let eave = $state(presets.eave_height);
  let overhang = $state(presets.overhang);
  let preview = $state<{ outline: [number, number][]; lines: [[number, number], [number, number]][]; area: number } | null>(null);
  let seq = 0;

  // Simple elevation sketches of each type (viewBox 0 0 64 40).
  const ICONS: Record<FormKind, string> = {
    gable: "M6 34V20L32 6l26 14v14z M6 20h52",
    hip: "M6 34V22l14-14h24l14 14v12z M6 22h52",
    half_hip: "M6 34V20l20-12h12l20 12v14z M26 8l-6 12M38 8l6 12M6 20h52",
    shed: "M6 34V14l52-8v28z M6 14l52-8",
    pyramid: "M10 34V22l22-16 22 16v12z M10 22h44M32 6v16",
    polygon: "M8 34V22l10-2 14-14 14 14 10 2v12z M8 22h48M18 20l14-14M46 20L32 6",
  };

  /** A regular polygon of eaves with the first wall along the x axis, corners at x, y >= 0. */
  function polygonPart(): RoofPart {
    const pts = regularPolygon({ x: 0, y: 0 }, sides, length);
    const [mx, my] = [Math.min(...pts.map((p) => p.x)), Math.min(...pts.map((p) => p.y))];
    return {
      name: "",
      edges: pts.map((p) => ({
        start: { x: +(p.x - mx).toFixed(3), y: +(p.y - my).toFixed(3) },
        pitch_deg: pitch,
        eave_height: eave,
        overhang,
      })),
    };
  }

  /** Fields whose typed text cannot be used, with the reason (from the field). */
  let invalid = $state<Record<string, string>>({});
  /** Why the preview could not be calculated for valid-looking values. */
  let refused = $state<string | null>(null);

  /** Records a field's validity (its label and reason) for the hint and the OK button. */
  const validity = (label: string) => (why: string | null) => {
    if (why === null) delete invalid[label];
    else invalid[label] = why;
  };

  /** Why OK / Create is not possible now, shown in place of the preview; null when it is. */
  const why = $derived.by(() => {
    const bad = Object.entries(invalid)[0];
    if (bad) return `${bad[0]}: ${bad[1]}`;
    return refused;
  });

  $effect(() => {
    const params = { kind: kind as StandardKind, length, width, pitchDeg: pitch, eaveHeight: eave, overhang };
    const poly = kind === "polygon";
    const ok = length > 0 && (poly ? sides >= 3 && sides <= 64 : width > 0) && pitch > 0 && pitch < 90;
    const typing = Object.keys(invalid).length > 0;
    const my = ++seq;
    void sides;
    refused = null;
    if (!ok || typing) {
      // No roof and no preview for values that are not in the fields: the
      // old preview would claim an area the dialog would not create.
      part = null;
      preview = null;
      return;
    }
    (poly ? Promise.resolve(polygonPart()) : api.standardPart(params)).then(async (p) => {
      if (my !== seq) return;
      const project = emptyProject("preview");
      project.parts = [p];
      const r = await api.calc(project);
      if (my !== seq) return;
      // Only a part the calculation took may be created.
      part = p;
      preview = {
        outline: p.edges.map((e) => [e.start.x, e.start.y]),
        lines: r.parts[0].geometry.lines.filter((l) => l.kind !== "eave").map((l) => [[l.a[0], l.a[1]], [l.b[0], l.b[1]]]),
        area: r.totals.roof_area,
      };
    }).catch((e) => {
      if (my !== seq) return;
      // No preview rather than a stale one, and no roof to create: the
      // reason stays in the dialog. A refusal of the values themselves is
      // what the dialog shows; anything else is also reported as an error.
      part = null;
      preview = null;
      refused = describeError(e, "preview").message;
      if (toAppError(e).code !== "out_of_range") notifyError(e, "preview");
    });
  });
</script>

<div class="form">
  <div class="types" role="radiogroup" aria-label={t("std.type")}>
    {#each KINDS as k}
      <button class="type" class:on={kind === k} role="radio" aria-checked={kind === k} onclick={() => (kind = k)}>
        <svg viewBox="0 0 64 40" aria-hidden="true"><path d={ICONS[k]} /></svg>
        <span>{t(`std.${k}`)}</span>
      </button>
    {/each}
  </div>
  <div class="row">
    <div class="fields">
      {#if kind === "polygon"}
        <label><span>{t("std.sides")}</span><input type="text" use:numeric={{ value: sides, step: 1, min: 3, max: 64, live: true, onchange: (v) => (sides = v), onvalidity: validity(t("std.sides")) }} /></label>
        <label><span>{t("std.side")}</span><div class="unit"><input type="text" use:numeric={{ value: length, step: 0.1, min: 0.1, max: MAX_SIDE, live: true, onchange: (v) => (length = v), onvalidity: validity(t("std.side")) }} /><em>m</em></div></label>
      {:else}
        <label><span>{t("std.length")}</span><div class="unit"><input type="text" use:numeric={{ value: length, step: 0.1, min: 0.1, max: MAX_SIDE, live: true, onchange: (v) => (length = v), onvalidity: validity(t("std.length")) }} /><em>m</em></div></label>
        <label class:off={kind === "pyramid"}><span>{t("std.width")}</span><div class="unit"><input type="text" disabled={kind === "pyramid"} use:numeric={{ value: width, step: 0.1, min: 0.1, max: MAX_SIDE, live: true, onchange: (v) => (width = v), onvalidity: validity(t("std.width")) }} /><em>m</em></div></label>
      {/if}
      <label><span>{t("edge.pitch")}</span><div class="unit"><input type="text" use:numeric={{ value: pitch, step: 1, min: 0.1, max: 89.9, live: true, onchange: (v) => (pitch = v), onvalidity: validity(t("edge.pitch")) }} /><em>°</em></div></label>
      <label><span>{t("edge.eaveHeight")}</span><div class="unit"><input type="text" use:numeric={{ value: eave, step: 0.05, min: -100, max: 1000, live: true, onchange: (v) => (eave = v), onvalidity: validity(t("edge.eaveHeight")) }} /><em>m</em></div></label>
      <label><span>{t("edge.overhang")}</span><div class="unit"><input type="text" use:numeric={{ value: overhang, step: 0.05, min: 0, max: 20, live: true, onchange: (v) => (overhang = v), onvalidity: validity(t("edge.overhang")) }} /><em>m</em></div></label>
    </div>
    <div class="preview">
      {#if preview}
        <PlanThumb outlines={[preview.outline]} lines={preview.lines} padding={0.1} />
        <span class="area">{m2(preview.area)}</span>
      {:else if why}
        <p class="why" role="status"><span class="mark" aria-hidden="true"></span>{why}</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .types {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 8px;
  }
  .type {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 10px 6px 8px;
    border: 1.5px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-2);
  }
  .type svg {
    width: 56px;
    height: 36px;
  }
  .type path {
    fill: var(--plan-fill);
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linejoin: round;
  }
  .type:hover {
    border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
  }
  .type.on {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 200px;
    gap: 16px;
  }
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 12px;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  label.off {
    opacity: 0.4;
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 100%;
    padding-right: 26px;
  }
  .unit em {
    position: absolute;
    right: 10px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
  }
  .preview {
    position: relative;
    height: 190px;
    padding: 12px;
    border-radius: 10px;
    background:
      linear-gradient(var(--plan-grid) 1px, transparent 1px) 0 0 / 14px 14px,
      linear-gradient(90deg, var(--plan-grid) 1px, transparent 1px) 0 0 / 14px 14px,
      var(--surface-2);
  }
  /* Why there is no roof: minimal and technical (AGENTS.md, error UI). */
  .why {
    display: flex;
    gap: 7px;
    align-items: baseline;
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 12px;
    color: var(--text-2);
  }
  .why .mark {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--danger);
  }
  .area {
    position: absolute;
    right: 8px;
    bottom: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
</style>
