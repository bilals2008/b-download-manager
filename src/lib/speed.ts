/**
 * Transfer rate and time left.
 *
 * The engine reports absolute byte counts, so the rate is the difference
 * between two reports. One sample per download is kept by the caller; the
 * exponential smoothing keeps a column from flickering between events.
 *
 * Framework-free and imported by the Node gate, so it names no React or
 * workspace aliases.
 */

export interface SpeedSample {
  downloaded: number;
  /** Epoch millis of the sample. */
  at: number;
  /** Smoothed bytes per second. */
  speed: number;
  /** Whether the download was receiving when the sample was taken. */
  running: boolean;
}

/** How much of the new rate a sample takes, and how much of the old it keeps. */
export const NEW_RATE_SHARE = 0.35;

/**
 * Fold one engine report into the running rate.
 *
 * A download that is not receiving has no rate: its sample drops to zero
 * instead of showing the speed it had before it was paused. A report that
 * arrives with the same byte count keeps the old rate and lets the smoothing
 * bring it down.
 *
 * The first report after a pause is a fresh baseline, not the first sample of
 * a very slow transfer: otherwise the time the download spent stopped would
 * be counted as time spent receiving, and the rate would stay low for several
 * seconds after a resume. A report whose byte count went backwards belongs to
 * a restarted download, so it is a baseline in the same way.
 */
export function nextSpeedSample(
  previous: SpeedSample | undefined,
  downloaded: number,
  running: boolean,
  now: number,
): SpeedSample {
  let speed = 0;
  const resumed = previous !== undefined && !previous.running && running;
  const restarted = previous !== undefined && downloaded < previous.downloaded;

  if (running && previous && !resumed && !restarted) {
    if (now > previous.at && downloaded >= previous.downloaded) {
      const instant = ((downloaded - previous.downloaded) * 1000) / (now - previous.at);
      speed =
        previous.speed > 0
          ? previous.speed * (1 - NEW_RATE_SHARE) + instant * NEW_RATE_SHARE
          : instant;
    } else {
      // Two reports in the same millisecond: the old rate is still the best
      // answer until there is a delta to measure.
      speed = previous.speed;
    }
  }

  return { downloaded, at: now, speed, running };
}

/** Seconds left at this rate. Zero when the size or the rate is not known. */
export function etaSeconds(total: number, downloaded: number, speed: number): number {
  if (total <= 0 || speed <= 0) {
    return 0;
  }
  return Math.max(0, total - downloaded) / speed;
}
