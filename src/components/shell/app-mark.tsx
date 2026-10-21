/**
 * A stand-in application mark: download arrow into a tray. The real IDM icon is
 * a trademarked asset, so the clone draws its own instead of copying it.
 */
export function AppMark() {
  return (
    <svg
      className="size-4 shrink-0"
      viewBox="0 0 16 16"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <linearGradient id="mark-arrow" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--icon-amber-bright)" />
          <stop offset="1" stopColor="var(--icon-amber)" />
        </linearGradient>
        <linearGradient id="mark-tray" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--icon-green-bright)" />
          <stop offset="1" stopColor="var(--icon-green)" />
        </linearGradient>
      </defs>
      <path
        d="M8 1.6v6.6"
        stroke="url(#mark-arrow)"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
      <path
        d="M4.7 5.4 8 8.7l3.3-3.3"
        fill="none"
        stroke="url(#mark-arrow)"
        strokeWidth="2.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M2.4 11.2c1 2.2 3 3.4 5.6 3.4s4.6-1.2 5.6-3.4"
        fill="none"
        stroke="url(#mark-tray)"
        strokeWidth="2.4"
        strokeLinecap="round"
      />
    </svg>
  );
}
