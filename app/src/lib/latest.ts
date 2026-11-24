// Live figures a form asks the core for while the user types: the request
// goes out after a short pause, a later request makes an earlier answer
// obsolete (it is dropped), and a failure is reported, never swallowed.

export interface Latest<A extends unknown[]> {
  /** Asks again with `args`; the first ask goes out at once, later ones after `delay` ms. */
  ask: (...args: A) => void;
  /** Drops what is pending and any answer still on its way (the form closed). */
  stop: () => void;
}

export function latest<A extends unknown[], R>(
  fn: (...args: A) => Promise<R>,
  onAnswer: (r: R) => void,
  onError: (e: unknown) => void,
  delay = 150,
): Latest<A> {
  let n = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let first = true;
  const run = async (k: number, args: A) => {
    try {
      const r = await fn(...args);
      if (k === n) onAnswer(r);
    } catch (e) {
      if (k === n) onError(e);
    }
  };
  return {
    ask: (...args: A) => {
      const k = ++n;
      clearTimeout(timer);
      if (first) {
        first = false;
        void run(k, args);
      } else {
        timer = setTimeout(() => void run(k, args), delay);
      }
    },
    stop: () => {
      n++;
      clearTimeout(timer);
    },
  };
}

/**
 * Orders the answers of reads that can overtake each other (two file events
 * in a row, a slow read of an older version): `next()` before each request,
 * and its answer is applied only while `current(k)` holds, so an older
 * answer never replaces a newer one (audit 2026-11-24 ERR-1902, ERR-1904,
 * ERR-1905). `bump()` makes every answer still on its way obsolete (an
 * action that changed what they would show, the view closed).
 */
export interface Sequence {
  /** Starts a request; its number. */
  next: () => number;
  /** Whether no newer request was started (or bumped) since request `k`. */
  current: (k: number) => boolean;
  /** Drops every answer still on its way. */
  bump: () => void;
}

export function sequence(): Sequence {
  let n = 0;
  return {
    next: () => ++n,
    current: (k: number) => k === n,
    bump: () => {
      n++;
    },
  };
}
