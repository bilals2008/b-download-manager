import { APP_HOME_PAGE, APP_NAME, APP_TAGLINE } from "./brand.ts";
import { isDesktop } from "./errors.ts";
import { shareToSystem } from "./ipc.ts";
import { shareApp } from "./share.ts";

/**
 * Hand the application to the system's share window, or the link to the
 * clipboard.
 *
 * There is no box in between: the Help row, the toolbar button and the About
 * window all call this. Only a machine that can show neither the share window
 * nor the Web Share API falls back to the clipboard, and only a clipboard that
 * refuses reports anything — through the caller's own error channel.
 */
export function tellFriend(report: (message: string) => void): void {
  const system = isDesktop()
    ? () => shareToSystem(APP_NAME, APP_TAGLINE, APP_HOME_PAGE)
    : undefined;
  void shareApp(system).then((outcome) => {
    if (outcome === "failed") {
      report(
        `The invitation could not be shared or copied. The address is ${APP_HOME_PAGE}`,
      );
    }
  });
}
