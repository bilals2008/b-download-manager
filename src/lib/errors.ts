/** Whether this page is the desktop application rather than a browser tab. */
export function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** A failure in words a reader can act on. */
export function describeError(error: unknown): string {
  if (!isDesktop()) {
    return "Downloading works only in the desktop app.";
  }
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  // A command that refuses a start answers with an object, so the window can
  // show the path and the name it suggests instead of a flattened sentence.
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") {
      return message;
    }
  }
  return "The app did not answer.";
}
