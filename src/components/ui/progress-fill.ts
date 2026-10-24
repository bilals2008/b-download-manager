/**
 * The fill of a download meter, by the state it shows. The colours are theme
 * tokens, so the light appearance carries darker stops on pale surfaces. The
 * list row and the progress window read the same map, so the two never drift.
 */
export const PROGRESS_FILL = {
  active:
    "bg-linear-to-r from-progress-active-from via-progress-active-via to-progress-active-to",
  complete:
    "bg-linear-to-r from-progress-complete-from to-progress-complete-to",
  paused: "bg-linear-to-r from-progress-paused-from to-progress-paused-to",
  queued: "bg-icon-amber",
  stopped: "bg-chrome-muted",
  failed: "bg-linear-to-r from-progress-failed-from to-progress-failed-to",
} as const;

export type ProgressFillState = keyof typeof PROGRESS_FILL;

/**
 * The background size a fill needs for its gradient to span the whole track.
 * The fill is `percent`% of the track, so its background is 100/percent of its
 * own width. A fill of nothing keeps the plain gradient.
 */
export function gradientSpan(percent: number): number {
  return percent > 0 ? 10_000 / percent : 100;
}
