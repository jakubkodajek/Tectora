/** The view box of a plan thumbnail: the extent of every outline point plus
 * `padding` of its larger side. A loop, not `Math.min(...points)`: a spread
 * over a job's points throws "Maximum call stack size exceeded" once a card
 * carries a few hundred thousand of them, which emptied the whole library
 * screen (audit 2026-11-22 ARCH-402). */
export function thumbBox(
  outlines: readonly (readonly (readonly [number, number])[])[],
  padding: number,
): { x: number; y: number; w: number; h: number } {
  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const o of outlines) {
    for (const p of o) {
      const [x, y] = p;
      if (!Number.isFinite(x) || !Number.isFinite(y)) continue;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
  }
  if (minX > maxX) return { x: 0, y: 0, w: 1, h: 1 };
  const size = Math.max(maxX - minX, maxY - minY) || 1;
  const pad = size * padding;
  return { x: minX - pad, y: minY - pad, w: maxX - minX + 2 * pad, h: maxY - minY + 2 * pad };
}
