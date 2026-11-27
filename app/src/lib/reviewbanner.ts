import type { ActivityItem, Change, ReviewStatus } from "./types";

/**
 * When the editor calculates the roof before the review's changes again:
 * the review's start and step count, and `before_rev`, which moves with the
 * user's own edits of the baseline while the other two stay (audit
 * 2026-11-24 ARCH-2004). Empty without a version before (an AI's new job).
 */
export function beforeKey(review: ReviewStatus | null): string {
  return review?.before ? `${review.since_ts_ms}:${review.count}:${review.before_rev ?? ""}` : "";
}

/**
 * The banner's change line and the plan's marks: the core's `changes`, the
 * approved state against the job now, the pair the figures use (owner
 * decision 4 of round 21, UX-2001); each step's own change stays in the AI
 * tab. An older bridge without them: the listed steps' changes.
 */
export function bannerChanges(review: ReviewStatus | null, steps: ActivityItem[]): Change[] {
  return review?.changes ?? steps.flatMap((a) => a.changes);
}
