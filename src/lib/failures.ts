/**
 * Why a download stopped, in words a user can act on.
 *
 * The engine reports the sentence of the failure — "the server refused the
 * request (403)", "not enough space on the disk: ..." — which is exact but not
 * friendly. This maps it onto a headline and one hint, and keeps the engine's
 * sentence for the technical details the window can unfold.
 */

export type FailureKind =
  | "network"
  | "refused"
  | "missing"
  | "busy"
  | "server"
  | "full"
  | "permission"
  | "destination"
  | "changed"
  | "size"
  | "unknown";

export interface FailureExplanation {
  kind: FailureKind;
  /** One sentence: what happened. */
  headline: string;
  /** What to do about it. */
  hint: string;
  /**
   * Whether trying the same destination again can work. A file that appeared
   * meanwhile is the case where it cannot: only another location helps.
   */
  retryable: boolean;
}

const UNKNOWN: FailureExplanation = {
  kind: "unknown",
  headline: "The download could not finish.",
  hint: "Retrying picks up where it stopped.",
  retryable: true,
};

/** Map the engine's sentence onto a headline and a hint. */
export function explainFailure(error: string | null): FailureExplanation {
  const text = (error ?? "").trim().toLowerCase();
  if (!text) {
    return UNKNOWN;
  }

  // A file that appeared while the transfer ran is never replaced: the start
  // was allowed because the name was free, so only a new location helps.
  if (text.includes("appeared while the download was running")) {
    return {
      kind: "changed",
      headline: "Another program saved a file with this name.",
      hint: "It was left alone. Choose another location to keep both files.",
      retryable: false,
    };
  }

  if (text.includes("not enough space") || text.includes("disk is full")) {
    return {
      kind: "full",
      headline: "There is not enough space left.",
      hint: "Free some space, or choose another location.",
      retryable: true,
    };
  }

  if (
    text.includes("permission denied") ||
    text.includes("access is denied") ||
    text.includes("os error 5") ||
    text.includes("os error 13") ||
    text.includes("read-only file system") ||
    text.includes("operation not permitted")
  ) {
    return {
      kind: "permission",
      headline: "The app could not write to that folder.",
      hint: "Choose another location, or close the program that is using it.",
      retryable: true,
    };
  }

  // A proxy that wants a sign-in is not the file's server refusing access,
  // and the answer is in Options, not on the page the link came from.
  if (text.includes("(407)") || text.includes("the server answered 407")) {
    return {
      kind: "refused",
      headline: "The proxy asked for a sign-in.",
      hint: "Check the proxy address in Options, or sign in to it.",
      retryable: true,
    };
  }

  if (
    text.includes("not on the server") ||
    text.includes("gone for good") ||
    text.includes("(404)") ||
    text.includes("(410)")
  ) {
    return {
      kind: "missing",
      headline: "The file is no longer at that address.",
      hint: "Open the page it came from and copy a fresh link.",
      retryable: true,
    };
  }

  if (
    text.includes("refused the request") ||
    text.includes("asked us to sign in") ||
    text.includes("(401)") ||
    text.includes("(403)")
  ) {
    return {
      kind: "refused",
      headline: "The site refused the download.",
      hint: "The link may have expired. Open the page again and copy a fresh one.",
      retryable: true,
    };
  }

  if (text.includes("server is busy") || text.includes("(429)")) {
    return {
      kind: "busy",
      headline: "The site is busy.",
      hint: "Wait a moment and try again. What has arrived so far is kept.",
      retryable: true,
    };
  }

  if (/\(5\d\d\)/.test(text) || text.includes("the server had a problem")) {
    return {
      kind: "server",
      headline: "The site had a problem.",
      hint: "Try again in a moment; the file is still there.",
      retryable: true,
    };
  }

  if (
    text.includes("the server promised") ||
    text.includes("shorter than the range") ||
    text.includes("assembled file") ||
    // The range the download asked for is not the range the file has any
    // more: the file changed on the server, so the parts cannot be joined.
    text.includes("(416)")
  ) {
    return {
      kind: "size",
      headline: "The download came up short.",
      hint: "Start it again to fetch the whole file.",
      retryable: true,
    };
  }

  if (
    text.includes("is not a folder") ||
    text.includes("not a valid address") ||
    text.includes("the url has no host") ||
    text.includes("the folder") ||
    text.includes("no such file") ||
    text.includes("cannot create") ||
    text.includes("cannot open")
  ) {
    return {
      kind: "destination",
      headline: "The save location cannot be used.",
      hint: "Choose another location for the file.",
      retryable: true,
    };
  }

  if (
    text.includes("error sending request") ||
    text.includes("dns") ||
    text.includes("failed to connect") ||
    text.includes("connection") ||
    text.includes("timed out") ||
    text.includes("timeout") ||
    text.includes("unreachable") ||
    text.includes("no route") ||
    text.includes("certificate") ||
    text.includes("tls") ||
    text.includes("proxy") ||
    text.includes("broken pipe") ||
    text.includes("unexpected eof")
  ) {
    return {
      kind: "network",
      headline: "The connection dropped.",
      hint: "Check your connection and try again. What has arrived so far is kept.",
      retryable: true,
    };
  }

  return UNKNOWN;
}

/** How much of the file arrived, in a sentence for the state row. */
export function receivedSummary(item: {
  downloaded: number;
  size: number;
  status: string;
}): string {
  if (item.status === "completed") {
    return "The file is complete.";
  }
  if (item.downloaded <= 0) {
    return "Nothing has arrived yet.";
  }
  if (item.size > 0) {
    return `${Math.min(100, Math.floor((item.downloaded / item.size) * 100))}% received`;
  }
  return "Part of the file arrived, but the site did not say how big it is.";
}

/** What Retry will do, in one line. */
export function resumeSummary(resumable: boolean | null): string {
  if (resumable === true) {
    return "Retrying picks up from what is already downloaded.";
  }
  if (resumable === false) {
    return "Retrying starts the file over from the beginning.";
  }
  return "The app works out whether it can pick up where it stopped.";
}
