import { APP_HOME_PAGE, APP_NAME, APP_TAGLINE } from "./brand.ts";
import { copyText } from "./clipboard.ts";

/** What telling a friend actually did, so the window can say it. */
export type ShareOutcome = "shared" | "copied" | "dismissed" | "failed";

const INVITATION = `${APP_NAME}: ${APP_TAGLINE} ${APP_HOME_PAGE}`;

/**
 * Hand the application to the share window, or the link to the clipboard.
 *
 * `system` is the desktop's own share window; the webview has none of its own,
 * so the shell command is passed in. The browser preview answers with the Web
 * Share API, and anything left over copies the message.
 */
export async function shareApp(
  system?: () => Promise<boolean>,
): Promise<ShareOutcome> {
  if (system && (await system())) {
    return "shared";
  }

  if (typeof navigator.share === "function") {
    try {
      await navigator.share({
        title: APP_NAME,
        text: APP_TAGLINE,
        url: APP_HOME_PAGE,
      });
      return "shared";
    } catch (failure) {
      // Closing the sheet without choosing anything is not a failure.
      if (failure instanceof DOMException && failure.name === "AbortError") {
        return "dismissed";
      }
      // No target installed, or the webview refused: the link is still copyable.
    }
  }

  return (await copyText(INVITATION)) ? "copied" : "failed";
}
