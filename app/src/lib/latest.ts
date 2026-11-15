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
