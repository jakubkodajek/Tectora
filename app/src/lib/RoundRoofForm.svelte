<script lang="ts">
  // DACAD's Runddach dialog: span, overhang and eave come from the two walls;
  // the arc is a semicircle or is given by radius, ridge height or developed
  // length. A cross-section shows the polygon that will be built.
  import { num } from "./format";
  import { t } from "./i18n.svelte";
  import { arcFor, minRadius, type Arc, type ArcMode } from "./roundroof";

  let {
    span,
    eave = $bindable(),
    overhang = $bindable(),
    arc = $bindable(null),
  }: { span: number; eave: number; overhang: number; arc: Arc | null } = $props();

  let mode = $state<ArcMode>("half");
  // Provisional default until DACAD runs show its segment count [no source].
  let segments = $state(8);
  let value = $state(0);

  const r0 = $derived(minRadius(span, overhang));
  // Suggest the semicircle's value whenever the mode changes.
  function setMode(m: ArcMode) {
    mode = m;
    value = m === "radius" ? +r0.toFixed(3) : m === "ridge" ? +(eave + r0).toFixed(3) : m === "length" ? +(Math.PI * r0).toFixed(3) : 0;
  }

  $effect(() => {
    arc = arcFor({ span, overhang, eave_height: eave, segments: Math.round(segments), mode, value });
  });

  // Cross-section preview (viewBox 0 0 200 120), walls at the bottom.
  const shape = $derived.by(() => {
    if (!arc) return null;
    const w = span / 2 + overhang;
    const top = arc.ridge;
    const s = Math.min(180 / (2 * w), 100 / Math.max(0.1, top - (eave - 1)));
    const X = (x: number) => 100 + x * s;
    const Y = (z: number) => 112 - (z - (eave - 1)) * s;
    const pts: [number, number][] = [[-w, eave]];
    let x = -w;
    let z = eave;
    const pitches = [arc.pitch_deg, ...arc.breaks.map((b) => b.pitch_deg)];
    const heights = [...arc.breaks.map((b) => b.height), top];
    pitches.forEach((p, k) => {
      const dz = heights[k] - z;
      x += dz / Math.tan((p * Math.PI) / 180);
      z = heights[k];
      pts.push([x, z]);
    });
    const left = pts.map(([px, pz]) => `${X(px)},${Y(pz)}`);
    const right = pts.slice(0, -1).reverse().map(([px, pz]) => `${X(-px)},${Y(pz)}`);
    return {
      roof: "M" + left.concat(right).join("L"),
      walls: `M${X(-span / 2)},112V${Y(eave)}M${X(span / 2)},112V${Y(eave)}`,
    };
  });
</script>

<div class="form">
  <div class="row">
    <div class="fields">
      <p class="tip">{t("round.span", { s: num(span) })}</p>
      <label><span>{t("edge.eaveHeight")}</span><div class="unit"><input type="number" step="0.05" bind:value={eave} /><em>m</em></div></label>
      <label><span>{t("edge.overhang")}</span><div class="unit"><input type="number" step="0.05" min="0" bind:value={overhang} /><em>m</em></div></label>
      <label>
        <span>{t("round.mode")}</span>
        <select value={mode} onchange={(e) => setMode((e.target as HTMLSelectElement).value as ArcMode)}>
          <option value="half">{t("round.half")}</option>
          <option value="radius">{t("round.radius")}</option>
          <option value="ridge">{t("round.ridge")}</option>
          <option value="length">{t("round.length")}</option>
        </select>
      </label>
      {#if mode !== "half"}
        <label>
          <span>{t(`round.${mode}`)}</span>
          <div class="unit"><input type="number" step="0.05" bind:value /><em>m</em></div>
        </label>
      {/if}
      <label><span>{t("round.segments")}</span><input type="number" step="1" min="1" max="30" bind:value={segments} /></label>
    </div>
    <div class="preview">
      {#if shape && arc}
        <svg viewBox="0 0 200 120" aria-hidden="true">
          <path class="walls" d={shape.walls} />
          <path class="roof" d={shape.roof} />
        </svg>
        <dl>
          <dt>{t("round.radius")}</dt><dd>{num(arc.radius)} m</dd>
          <dt>{t("round.ridge")}</dt><dd>{num(arc.ridge)} m</dd>
          <dt>{t("round.length")}</dt><dd>{num(arc.length)} m</dd>
        </dl>
      {:else}
        <p class="err">{t("round.invalid", { r: num(r0) })}</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .row {
    display: flex;
    gap: 20px;
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 220px;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 100%;
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
  select {
    width: 100%;
    height: 34px;
  }
  .preview {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  svg {
    width: 100%;
    background: var(--plan-fill);
    border-radius: 8px;
  }
  .roof {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.6;
    stroke-linejoin: round;
  }
  .walls {
    stroke: var(--text-2);
    stroke-width: 1.4;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 0;
    font-size: 13px;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .tip,
  .err {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .err {
    color: var(--danger);
  }
</style>
