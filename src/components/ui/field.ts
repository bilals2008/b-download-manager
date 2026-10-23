/**
 * The box every text field in the application uses.
 *
 * Fields are the one place a border is the affordance: a panes and rows
 * separate by surface and space, a field has to show where typing goes.
 */
export const FIELD =
  "w-full rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 py-1.5 text-xs text-chrome-menu-text transition-colors placeholder:text-chrome-muted/70 hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:border-chrome-line disabled:text-chrome-muted disabled:hover:border-chrome-line";
