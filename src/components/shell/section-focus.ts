/**
 * The section cycling behind "Move to the next section".
 *
 * The three places the keyboard lives in — the menu bar, the categories pane
 * and the download list — each carry `data-shortcut-section`. The order in
 * the document is the order the key walks, so a hidden categories pane drops
 * out of the cycle without any special case.
 */

const SECTION_SELECTOR = "[data-shortcut-section]";

/**
 * Step the keyboard to the next (or previous) section and focus its own
 * first stop: the roving button or the element that takes the tab itself.
 */
export function cycleSectionFocus(step: 1 | -1): void {
  const sections = Array.from(
    document.querySelectorAll<HTMLElement>(SECTION_SELECTOR),
  );
  if (sections.length === 0) {
    return;
  }
  const active =
    document.activeElement instanceof Element
      ? document.activeElement.closest(SECTION_SELECTOR)
      : null;
  const index = active ? sections.indexOf(active as HTMLElement) : -1;
  const next = sections[mod(index + step, sections.length)];
  focusInside(next);
}

function focusInside(section: HTMLElement): void {
  const stop =
    section.tabIndex === 0
      ? section
      : (section.querySelector<HTMLElement>('[tabindex="0"]') ??
        section.querySelector<HTMLElement>("button"));
  (stop ?? section).focus();
}

/** A remainder that stays positive for negative numbers too. */
function mod(value: number, length: number): number {
  return ((value % length) + length) % length;
}
