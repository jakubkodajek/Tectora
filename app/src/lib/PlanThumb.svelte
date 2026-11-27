<script lang="ts">
  import { thumbBox } from "./thumbbox";

  let {
    outlines,
    lines = [],
    padding = 0.08,
  }: { outlines: [number, number][][]; lines?: [[number, number], [number, number]][]; padding?: number } = $props();

  const box = $derived(thumbBox(outlines, padding));

  // Plan y points north (up); SVG y points down.
  const pathFor = (o: [number, number][]) => "M" + o.map((p) => `${p[0]},${-p[1]}`).join("L") + "Z";
</script>

<svg viewBox="{box.x} {-(box.y + box.h)} {box.w} {box.h}" preserveAspectRatio="xMidYMid meet">
  {#each outlines as o}
    <path d={pathFor(o)} vector-effect="non-scaling-stroke" />
  {/each}
  {#each lines as l}
    <line x1={l[0][0]} y1={-l[0][1]} x2={l[1][0]} y2={-l[1][1]} vector-effect="non-scaling-stroke" />
  {/each}
</svg>

<style>
  svg {
    width: 100%;
    height: 100%;
    display: block;
  }
  path {
    fill: var(--plan-fill);
    stroke: var(--accent);
    stroke-width: 1.6;
    stroke-linejoin: round;
  }
  line {
    stroke: var(--accent);
    stroke-width: 1.1;
    opacity: 0.75;
  }
</style>
