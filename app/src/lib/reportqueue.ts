// The bug report outbox as the UI keeps it: counts for the indicator, the
// rows of the list and whether this build can send at all. Read from the app
// at start and again whenever it says the queue changed.
import { api } from "./api";
import { report, setOutboxCounts } from "./report.svelte";

/** Reads the queue; a failure leaves what is shown as it is (the queue is never a notice). */
export async function refreshOutbox(): Promise<void> {
  try {
    const o = await api.bugReportOutbox();
    report.sendable = o.sendable === true;
    setOutboxCounts(o.counts);
    report.queue = Array.isArray(o.reports) ? o.reports : [];
  } catch {
    // Offline-first: nothing to tell.
  }
}
