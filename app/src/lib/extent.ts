// Smallest and largest of many numbers. A spread of a list into Math.min or
// Math.max passes every value as an argument and throws "Maximum call stack
// size exceeded" (a RangeError) for long lists; a job may have 100 000s of
// corners within its limits (hand-drawn faces, round roofs), and one throw
// empties a whole screen (review round 5 ARCH-402). So every list of
// coordinates goes through these loops.

/** The smallest of `xs` (of `f` of each), Infinity for none, like `Math.min()`. */
export function minOf<T>(xs: readonly T[], f: (x: T) => number = (x) => x as unknown as number): number {
  let m = Infinity;
  for (const x of xs) {
    const v = f(x);
    if (v < m || Number.isNaN(v)) m = v;
    if (Number.isNaN(m)) return NaN;
  }
  return m;
}

/** The largest of `xs` (of `f` of each), -Infinity for none, like `Math.max()`. */
export function maxOf<T>(xs: readonly T[], f: (x: T) => number = (x) => x as unknown as number): number {
  let m = -Infinity;
  for (const x of xs) {
    const v = f(x);
    if (v > m || Number.isNaN(v)) m = v;
    if (Number.isNaN(m)) return NaN;
  }
  return m;
}

/** Appends every item of `items` to `list` (a spread into `push` throws for long lists, as for Math.max); returns `list`. */
export function pushAll<T>(list: T[], items: readonly T[]): T[] {
  for (const x of items) list.push(x);
  return list;
}
