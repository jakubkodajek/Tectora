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
