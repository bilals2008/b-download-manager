/** Messages from the Tauri engine, in words a user can act on. */
export function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function describeError(error: unknown): string {
  if (!isDesktop()) {
    return "The download engine is only available in the desktop window.";
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
  return "The download engine did not answer.";
}
