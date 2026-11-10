// Work that is still running when the user acts again: field handlers that
// ask the core before they change the job (leaving must wait for them,
// ERR-004), and buttons whose second press must not run the action twice
// (UI-002).

/** Edits that await the core before they mark the job dirty. */
export class PendingEdits {
  private running = new Set<Promise<unknown>>();

  /** Registers `p` until it settles; returns it unchanged. */
  track<T>(p: Promise<T>): Promise<T> {
    this.running.add(p);
    const done = () => {
      this.running.delete(p);
    };
    p.then(done, done);
    return p;
  }

  get size(): number {
    return this.running.size;
  }

  /** Resolves once every tracked edit has settled, also ones started while waiting. Never rejects. */
  async settled(): Promise<void> {
    while (this.running.size) await Promise.allSettled([...this.running]);
  }
}

/**
 * Wraps `fn` so that a call while an earlier one still runs is ignored.
 * `onbusy` hears true when a run starts and false when it ends (to disable
 * the buttons meanwhile).
 */
export function exclusive<A extends unknown[]>(fn: (...args: A) => Promise<unknown>, onbusy: (busy: boolean) => void = () => {}): (...args: A) => Promise<void> {
  let busy = false;
  return async (...args: A) => {
    if (busy) return;
    busy = true;
    onbusy(true);
    try {
      await fn(...args);
    } finally {
      busy = false;
      onbusy(false);
    }
  };
}

/**
 * Browser noise that is not an error of the app: Chromium (WebView2 on
 * Windows) reports a ResizeObserver callback that resized again as a window
 * error. Nothing is lost; the observer runs again on the next frame (UI-019).
 */
export function isBenignError(e: unknown): boolean {
  const msg = typeof e === "string" ? e : e instanceof Error ? e.message : typeof (e as { message?: unknown })?.message === "string" ? (e as { message: string }).message : "";
  return /^ResizeObserver loop (completed with undelivered notifications|limit exceeded)/.test(msg);
}
