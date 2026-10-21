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
  return "The download engine did not answer.";
}
