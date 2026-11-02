import { useEffect } from "react";

/** The anchor a search result asked to open. A new object means a new jump. */
export interface SettingTarget {
  anchor: string;
}

/** How long the row keeps its ring after the jump. */
export const SETTING_FLASH_MS = 1600;

/**
 * Scroll to a searched setting once its page has drawn, put the keyboard on
 * it, and keep a ring around it long enough to be noticed. A DOM effect is the
 * right tool here: the element does not exist until the page state has
 * rendered, and the work is not part of any state value.
 */
export function useSettingReveal(
  target: SettingTarget | null,
  flashMs = SETTING_FLASH_MS,
): void {
  useEffect(() => {
    if (!target) {
      return;
    }

    let flashed: HTMLElement | null = null;
    let frame = 0;
    let animation = 0;
    let timer = 0;

    // The page can take a frame or two to commit, especially when the search
    // panel has just given way to it. Keep looking briefly rather than losing
    // the jump because the element was not there on the first frame.
    const reveal = () => {
      const element = document.getElementById(target.anchor);
      if (!element) {
        if (frame++ < 90) {
          animation = window.requestAnimationFrame(reveal);
        }
        return;
      }
      flashed = element;
      element.scrollIntoView?.({ block: "center" });
      element.classList.add("setting-flash");
      element.focus?.({ preventScroll: true });
      timer = window.setTimeout(() => {
        element.classList.remove("setting-flash");
      }, flashMs);
    };

    reveal();

    return () => {
      window.cancelAnimationFrame(animation);
      window.clearTimeout(timer);
      flashed?.classList.remove("setting-flash");
    };
  }, [target, flashMs]);
}
