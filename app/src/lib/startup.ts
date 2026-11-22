// Start-up of the app window.

/**
 * Runs `take` once every listener in `listeners` is registered (or failed
 * to register), so an event sent meanwhile is either heard or its item is
 * still waiting to be taken: a file opened at cold start whose import ended
 * between the take and the registration was neither (round-6 audit, errors
 * "To check" 75).
 */
export async function afterListening(listeners: Promise<unknown>[], take: () => void): Promise<void> {
  await Promise.allSettled(listeners);
  take();
}

/** What `startOpening` needs: the two events, the two takes, and what to do with a job or a failure. */
export interface OpeningDeps<E> {
  /** Registers the listener of "a file opened from the system is a job now"; resolves to its remover. */
  onOpenProject(heard: (id: string) => void): Promise<() => void>;
  /** Registers the listener of "a file opened from the system did not open". */
  onOpenFailed(heard: () => void): Promise<() => void>;
  /** The job opened before the window listened, if any (taken once). */
  takePendingOpen(): Promise<string | null | undefined>;
  /** The failures not shown yet (taken once each). */
  takeOpenErrors(): Promise<E[]>;
  open(id: string): void;
  failed(e: E): void;
}

/**
 * Files opened from the system (double-click on a .tectora / .tectorajob):
 * listens for both events and, only once both listeners are registered,
 * takes what came in before (`afterListening`). The takes live here and
 * nowhere else, so none can run before the listeners (TEST-604). Returns
 * the remover of the listeners and `ready`, resolved after the first take.
 */
export function startOpening<E>(deps: OpeningDeps<E>): { stop: () => void; ready: Promise<void> } {
  const showErrors = () =>
    deps.takeOpenErrors().then(
      (es) => es.forEach((e) => deps.failed(e)),
      () => {},
    );
  const unOpen = deps.onOpenProject(deps.open);
  // Told like a failed import, at start-up and later (ERR-006).
  const unFailed = deps.onOpenFailed(() => void showErrors());
  const ready = afterListening([unOpen, unFailed], () => {
    deps.takePendingOpen().then(
      (id) => id && deps.open(id),
      () => {},
    );
    void showErrors();
  });
  return {
    stop: () => {
      unOpen.then((f) => f(), () => {});
      unFailed.then((f) => f(), () => {});
    },
    ready,
  };
}
