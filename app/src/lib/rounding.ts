// Rounding of the editor operations, bit for bit as tectora-core's
// `edit::round_to`: `(v · 10^d).round() / 10^d`, halves away from zero.
// `toFixed` (exact decimal value, so 9.9995 stays 9.999) and `Math.round`
// (halves towards +∞, so -0.0005 becomes 0) differ from it, and the UI then
// stored other corners than an AI got over MCP for the same operation
// (`editparity.test.ts`).

export function roundTo(v: number, digits: number): number {
  const f = 10 ** digits;
  const x = v * f;
  return (x < 0 ? -Math.round(-x) : Math.round(x)) / f;
}
