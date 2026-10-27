import { invoke } from "@tauri-apps/api/core";

import { isDesktop } from "@/lib/errors";

/**
 * Open a web address in the browser the system uses.
 *
 * The window is a webview and not a tab: an ordinary anchor would replace the
 * page it sits on, so the address is handed to the shell, which opens it in
 * whatever browser the reader chose. In a plain-browser preview the page is
 * already in a browser, so a new tab is the same answer.
 *
 * Refusing an address that is not a web page is the command's job, not this
 * one's; a failure here is a machine with no browser to hand it to, which the
 * reader can work out from the tab that did not appear.
 */
export function openExternal(url: string): void {
  if (!isDesktop()) {
    window.open(url, "_blank", "noreferrer");
    return;
  }
  void invoke("open_url", { url }).catch(() => undefined);
}
