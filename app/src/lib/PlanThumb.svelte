<script lang="ts">
  let { outlines, padding = 0.08 }: { outlines: [number, number][][]; padding?: number } = $props();

  const box = $derived.by(() => {
    const pts = outlines.flat();
    if (pts.length === 0) return { x: 0, y: 0, w: 1, h: 1 };
    const xs = pts.map((p) => p[0]);
    const ys = pts.map((p) => p[1]);
    const minX = Math.min(...xs);
    const maxX = Math.max(...xs);
    const minY = Math.min(...ys);
    const maxY = Math.max(...ys);
    const size = Math.max(maxX - minX, maxY - minY) || 1;
    const pad = size * padding;
    return { x: minX - pad, y: minY - pad, w: maxX - minX + 2 * pad, h: maxY - minY + 2 * pad };
  });

  // Plan y points north (up); SVG y points down.
  const pathFor = (o: [number, number][]) => "M" + o.map((p) => `${p[0]},${-p[1]}`).join("L") + "Z";
</script>

<svg viewBox="{box.x} {-(box.y + box.h)} {box.w} {box.h}" preserveAspectRatio="xMidYMid meet">
  {#each outlines as o}
    <path d={pathFor(o)} vector-effect="non-scaling-stroke" />
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
</style>
