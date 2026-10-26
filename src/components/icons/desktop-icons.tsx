import { useId, type ReactNode } from "react";

import {
  CATEGORY_PALETTES,
  categoryPaint,
  defaultCategoryIcon,
  type CategoryGlyphId,
  type CategoryIcon,
  type CategoryPaint,
  type CategoryPalette,
} from "@/lib/category-icons";
import type { CategoryName, DownloadStatus } from "@/lib/downloads";

/**
 * The desktop artwork: file-type badges, folder icons, status marks and the
 * empty-state illustration.
 *
 * The style is the one the toolbar already uses, shrunk down: a coloured plate
 * with a two-stop gloss, a deep hairline, and a white glyph. Plate colours come
 * from the icon tokens, so the light and dark appearances follow the theme
 * without a second copy of the drawings.
 */

/** A paint stop is either a theme variable name or a colour already. */
const cssColor = (value: string) =>
  value.startsWith("--") ? `var(${value})` : value;

/** The white mark on a plate, drawn on the 24-unit grid. */
const GLYPHS: Record<
  CategoryGlyphId,
  (colors: CategoryPaint) => ReactNode
> = {
  folder: () => (
    <path d="M5.2 7.1a1.7 1.7 0 0 1 1.7-1.7h3.2l1.8 2h5.9a1.7 1.7 0 0 1 1.7 1.7v7.8a1.7 1.7 0 0 1-1.7 1.7H6.9a1.7 1.7 0 0 1-1.7-1.7z" />
  ),
  page: (colors) => (
    <g>
      <path d="M8.6 6.2h6.8v11.6H8.6z" />
      <path
        d="M12.2 6.4v2.7h2.8"
        fill="none"
        stroke={cssColor(colors.deep)}
        strokeWidth="1"
        strokeLinejoin="round"
      />
      <path
        d="M10.6 12.2h2.8M10.6 14.6h2.8"
        fill="none"
        stroke={cssColor(colors.deep)}
        strokeWidth="1"
        strokeLinecap="round"
      />
    </g>
  ),
  video: () => <path d="M9.8 7.1 16.6 12l-6.8 4.9z" />,
  music: () => (
    <g>
      <circle cx="9.2" cy="15.2" r="1.9" />
      <rect x="10.8" y="7.4" width="1.7" height="7.8" rx="0.4" />
      <path d="M10.8 7.4h4.4v2h-4.4z" />
    </g>
  ),
  document: (colors) => (
    <g>
      <path d="M8.4 6.2h7.2v11.6H8.4z" />
      <path
        d="M10.4 9.6h3.2M10.4 12.2h3.2M10.4 14.8h2.2"
        fill="none"
        stroke={cssColor(colors.deep)}
        strokeWidth="1.1"
        strokeLinecap="round"
      />
    </g>
  ),
  program: () => (
    <g>
      <rect x="6.6" y="6.6" width="4.2" height="4.2" rx="1" />
      <rect x="13.2" y="6.6" width="4.2" height="4.2" rx="1" />
      <rect x="6.6" y="13.2" width="4.2" height="4.2" rx="1" />
      <rect x="13.2" y="13.2" width="4.2" height="4.2" rx="1" />
    </g>
  ),
  game: (colors) => (
    <g>
      <path d="M7.7 8.7h8.6a3.2 3.2 0 0 1 3.12 3.9l-.6 2.95a2.3 2.3 0 0 1-4.02 1.1l-.98-1.2h-3.64l-.98 1.2a2.3 2.3 0 0 1-4.02-1.1l-.6-2.95a3.2 3.2 0 0 1 3.12-3.9z" />
      <path
        d="M9 10.5h1.3v1.3h1.3v1.3h-1.3v1.3H9v-1.3H7.7v-1.3H9z"
        fill={cssColor(colors.deep)}
      />
      <circle cx="14.6" cy="11.4" r=".85" fill={cssColor(colors.deep)} />
      <circle cx="16.2" cy="13.3" r=".85" fill={cssColor(colors.deep)} />
    </g>
  ),
  archive: (colors) => (
    <g>
      <path d="M7.2 6.8h9.6v10.2H7.2z" />
      <path
        d="M7.4 9.5h9.2M7.4 12.2h9.2M7.4 14.9h9.2"
        fill="none"
        stroke={cssColor(colors.deep)}
        strokeWidth="1"
        strokeLinecap="round"
      />
    </g>
  ),
  ellipsis: () => (
    <g>
      <circle cx="7.4" cy="12" r="1.15" />
      <circle cx="12" cy="12" r="1.15" />
      <circle cx="16.6" cy="12" r="1.15" />
    </g>
  ),
  image: (colors) => (
    <g>
      <rect x="5.2" y="6.2" width="13.6" height="11.6" rx="1.8" />
      <circle cx="9.2" cy="10.2" r="1.2" fill={cssColor(colors.deep)} />
      <path
        d="m6.6 16.4 3.5-3.8 2.3 2.4 2-2.2 3.1 3.6z"
        fill={cssColor(colors.deep)}
      />
    </g>
  ),
  code: () => (
    <g
      fill="none"
      stroke="#fff"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="m9.4 8.2-3.5 3.8 3.5 3.8" />
      <path d="m14.6 8.2 3.5 3.8-3.5 3.8" />
      <path d="M12.9 7.4 11.1 16.6" strokeWidth="1.4" />
    </g>
  ),
  book: (colors) => (
    <g>
      <path d="M6.5 6.3h4.3a2 2 0 0 1 1.2.4 2 2 0 0 1 1.2-.4h4.3v10.6h-4.3a2 2 0 0 0-1.2.4 2 2 0 0 0-1.2-.4H6.5z" />
      <path
        d="M12 6.9v10.2"
        fill="none"
        stroke={cssColor(colors.deep)}
        strokeWidth="1"
      />
    </g>
  ),
  cloud: () => (
    <path d="M9.1 17.3h7a2.8 2.8 0 0 0 .3-5.58 4.3 4.3 0 0 0-8.25-.93A3.4 3.4 0 0 0 9.1 17.3z" />
  ),
  globe: (colors) => (
    <g>
      <circle cx="12" cy="12" r="5.4" />
      <g fill="none" stroke={cssColor(colors.deep)} strokeWidth=".9">
        <ellipse cx="12" cy="12" rx="2.4" ry="5.4" />
        <path d="M6.7 12h10.6" />
      </g>
    </g>
  ),
  star: () => (
    <path d="m12 6.1 1.78 3.6 3.97.58-2.87 2.8.68 3.95L12 15.17l-3.56 1.86.68-3.95-2.87-2.8 3.97-.58z" />
  ),
  box: (colors) => (
    <g>
      <path d="m12 5.5 6.2 3.3v6.4L12 18.5l-6.2-3.3V8.8z" />
      <g
        fill="none"
        stroke={cssColor(colors.deep)}
        strokeWidth=".9"
        strokeLinejoin="round"
      >
        <path d="M5.8 8.8 12 12l6.2-3.2M12 12v6.5" />
      </g>
    </g>
  ),
};

/** The saved icon of a category, or the one its name has always had. */
function artFor(category: CategoryName, icon?: CategoryIcon): CategoryIcon {
  return icon ?? defaultCategoryIcon(category);
}

/** One gradient per drawing instance, so the gloss never leaks between rows. */function useArtIds(prefix: string): { front: string; back: string } {
  const id = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  return { front: `${prefix}-front-${id}`, back: `${prefix}-back-${id}` };
}

function Plate({
  front,
  deep,
  label,
}: {
  front: string;
  deep: string;
  label?: ReactNode;
}) {
  return (
    <g>
      <rect
        x="2"
        y="2"
        width="20"
        height="20"
        rx="4.6"
        fill={`url(#${front})`}
        stroke={cssColor(deep)}
        strokeOpacity=".55"
        strokeWidth=".9"
      />
      <rect x="3.4" y="3.2" width="17.2" height="6.2" rx="3.1" fill="#fff" opacity=".14" />
      {label}
    </g>
  );
}

function Gloss({
  id,
  bright,
  base,
}: {
  id: string;
  bright: string;
  base: string;
}) {
  return (
    <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" style={{ stopColor: cssColor(bright) }} />
      <stop offset="1" style={{ stopColor: cssColor(base) }} />
    </linearGradient>
  );
}

/** The colourful badge a download row, a picker or a category row shows. */
export function FileTypeIcon({
  category,
  icon,
  className,
}: {
  category: CategoryName;
  /** The saved icon of the category; omitted means the default for its name. */
  icon?: CategoryIcon;
  className?: string;
}) {
  const art = artFor(category, icon);
  const colors = categoryPaint(art);
  const ids = useArtIds("file");
  return (
    <svg
      viewBox="0 0 24 24"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id={ids.front} bright={colors.bright} base={colors.base} />
      </defs>
      <Plate
        front={ids.front}
        deep={colors.deep}
        label={
          <g
            fill={colors.ink}
            opacity=".96"
            stroke={cssColor(colors.deep)}
            strokeWidth=".9"
            strokeLinejoin="round"
          >
            {GLYPHS[art.glyph](colors)}
          </g>
        }
      />
    </svg>
  );
}

const CLOSED_FACE =
  "M3 9.6a2.2 2.2 0 0 1 2.2-2.2h13.6a2.2 2.2 0 0 1 2.2 2.2v8.2a2.2 2.2 0 0 1-2.2 2.2H5.2A2.2 2.2 0 0 1 3 17.8z";
const OPEN_FACE =
  "M4.4 8.4h15.2a1.4 1.4 0 0 1 1.37 1.7l-1.75 8a2.2 2.2 0 0 1-2.15 1.74H5.2A2.2 2.2 0 0 1 3 17.8v-7.2a2.2 2.2 0 0 1 1.4-2.2z";

/** A real folder, tab and all, tinted for its category. */
export function CategoryFolderIcon({
  category,
  icon,
  open = false,
  className,
}: {
  category: CategoryName;
  /** The saved icon of the category; omitted means the default for its name. */
  icon?: CategoryIcon;
  open?: boolean;
  className?: string;
}) {
  const art = artFor(category, icon);
  const colors = categoryPaint(art);
  const ids = useArtIds("folder");
  return (
    <svg
      viewBox="0 0 24 24"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id={ids.front} bright={colors.bright} base={colors.base} />
        <linearGradient id={ids.back} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" style={{ stopColor: cssColor(colors.base) }} />
          <stop offset="1" style={{ stopColor: cssColor(colors.deep) }} />
        </linearGradient>
      </defs>
      <path
        d="M3 6.4a2.2 2.2 0 0 1 2.2-2.2h4.1l2.1 2.4h7.4a2.2 2.2 0 0 1 2.2 2.2v9.2a2.2 2.2 0 0 1-2.2 2.2H5.2A2.2 2.2 0 0 1 3 17.8z"
        fill={`url(#${ids.back})`}
        stroke={cssColor(colors.deep)}
        strokeOpacity=".6"
        strokeWidth=".8"
      />
      <path
        d={open ? OPEN_FACE : CLOSED_FACE}
        fill={`url(#${ids.front})`}
        stroke={cssColor(colors.deep)}
        strokeOpacity=".55"
        strokeWidth=".8"
      />
      <path
        d="M4.6 10.8h14.8"
        stroke="#fff"
        strokeOpacity=".3"
        strokeWidth=".9"
        strokeLinecap="round"
      />
      <g
        fill={colors.ink}
        opacity=".96"
        stroke={cssColor(colors.deep)}
        strokeWidth="1.1"
        strokeLinejoin="round"
        transform="translate(12 14.9) scale(.62) translate(-12 -12)"
      >
        {GLYPHS[art.glyph](colors)}
      </g>
    </svg>
  );
}

const VIEW_PALETTES = {
  all: CATEGORY_PALETTES.blue,
  unfinished: CATEGORY_PALETTES.amber,
  finished: CATEGORY_PALETTES.green,
} as const;

export type ViewKind = keyof typeof VIEW_PALETTES;

/** The plate of All Downloads, Unfinished and Finished. */
export function ViewIcon({
  view,
  className,
}: {
  view: ViewKind;
  className?: string;
}) {
  const palette = VIEW_PALETTES[view];
  const ids = useArtIds("view");
  return (
    <svg
      viewBox="0 0 24 24"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <Gloss id={ids.front} bright={palette.bright} base={palette.base} />
      </defs>
      <Plate
        front={ids.front}
        deep={palette.deep}
        label={
          <g
            fill="#fff"
            opacity=".96"
            stroke={cssColor(palette.deep)}
            strokeWidth=".9"
            strokeLinejoin="round"
          >
            {view === "all" ? (
              <>
                <path d="M10.7 5.4h2.6v4.9h2.6L12 15.2l-3.9-4.9h2.6z" />
                <path d="M5.4 15.8h13.2v1.7a2 2 0 0 1-2 2H7.4a2 2 0 0 1-2-2z" />
              </>
            ) : view === "unfinished" ? (
              <>
                <circle cx="12" cy="12" r="5.1" />
                <path
                  d="M12 8.7V12l2.5 1.6"
                  fill="none"
                  stroke={cssColor(palette.deep)}
                  strokeWidth="1.7"
                  strokeLinecap="round"
                />
              </>
            ) : (
              <path d="m6.5 12.4 2.2-2.2 2.6 2.6 5.1-5.3 2.2 2.2-7.3 7.5z" />
            )}
          </g>
        }
      />
    </svg>
  );
}

const STATUS_PALETTES: Record<DownloadStatus, CategoryPalette> = {
  downloading: CATEGORY_PALETTES.green,
  queued: CATEGORY_PALETTES.amber,
  paused: CATEGORY_PALETTES.blue,
  stopped: CATEGORY_PALETTES.silver,
  completed: CATEGORY_PALETTES.green,
  error: CATEGORY_PALETTES.red,
};

/** The round status chip on a download row. */
export function StatusMark({
  status,
  className,
}: {
  status: DownloadStatus;
  className?: string;
}) {
  const palette = STATUS_PALETTES[status];

  const glyph = (() => {
    switch (status) {
      case "downloading":
        return (
          <>
            <path
              d="M8 4.4v6.2"
              stroke="#fff"
              strokeWidth="1.7"
              strokeLinecap="round"
            />
            <path
              d="m5.4 8.3 2.6 2.6 2.6-2.6"
              fill="none"
              stroke="#fff"
              strokeWidth="1.7"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </>
        );
      case "completed":
        return (
          <path
            d="m5.1 8.2 2 2 3.9-4.3"
            fill="none"
            stroke="#fff"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        );
      case "queued":
        return (
          <>
            <circle
              cx="8"
              cy="8"
              r="2.9"
              fill="none"
              stroke="#fff"
              strokeWidth="1.5"
            />
            <path
              d="M8 6.5V8l1.6 1"
              fill="none"
              stroke="#fff"
              strokeWidth="1.5"
              strokeLinecap="round"
            />
          </>
        );
      case "paused":
        return (
          <>
            <rect x="5.6" y="5.3" width="1.7" height="5.4" rx=".5" fill="#fff" />
            <rect x="8.7" y="5.3" width="1.7" height="5.4" rx=".5" fill="#fff" />
          </>
        );
      case "stopped":
        return <rect x="5.7" y="5.7" width="4.6" height="4.6" rx=".9" fill="#fff" />;
      default:
        return (
          <>
            <rect x="7.25" y="4.5" width="1.5" height="5" rx=".75" fill="#fff" />
            <circle cx="8" cy="11.5" r=".9" fill="#fff" />
          </>
        );
    }
  })();

  return (
    <svg
      viewBox="0 0 16 16"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      <circle cx="8" cy="8" r="6.9" fill={cssColor(palette.deep)} />
      <circle
        cx="8"
        cy="8"
        r="6.2"
        fill="none"
        stroke={`var(${palette.bright})`}
        strokeOpacity=".55"
        strokeWidth=".9"
      />
      <ellipse cx="8" cy="4.8" rx="4.4" ry="1.4" fill="#fff" opacity=".16" />
      {glyph}
    </svg>
  );
}

/** A warning plate for the error banner and the dialog. */
export function WarningMark({
  tone = "amber",
  className,
}: {
  tone?: "amber" | "red";
  className?: string;
}) {
  const palette =
    tone === "red" ? CATEGORY_PALETTES.red : CATEGORY_PALETTES.amber;
  return (
    <svg
      viewBox="0 0 16 16"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      <path
        d="M8 1.9 14.9 13.5a.9.9 0 0 1-.77 1.35H1.87a.9.9 0 0 1-.77-1.35z"
        fill={cssColor(palette.deep)}
        stroke={`var(${palette.bright})`}
        strokeOpacity=".55"
        strokeWidth="1"
        strokeLinejoin="round"
      />
      <path
        d="M8 5.3v4.2"
        stroke="#fff"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <circle cx="8" cy="11.7" r=".85" fill="#fff" />
    </svg>
  );
}

/** The empty list: an arrow dropping into a tray, in the toolbar's language. */
export function EmptyDownloadsArt({ className }: { className?: string }) {
  const id = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const tray = `tray-${id}`;
  const arrow = `arrow-${id}`;
  return (
    <svg
      viewBox="0 0 32 32"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <linearGradient id={tray} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" style={{ stopColor: "var(--icon-silver-bright)" }} />
          <stop offset="1" style={{ stopColor: "var(--icon-silver)" }} />
        </linearGradient>
        <linearGradient id={arrow} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" style={{ stopColor: "var(--icon-green-bright)" }} />
          <stop offset="1" style={{ stopColor: "var(--icon-green)" }} />
        </linearGradient>
      </defs>
      <path
        d="M16 4.2v11.2"
        stroke={`url(#${arrow})`}
        strokeWidth="3.6"
        strokeLinecap="round"
      />
      <path
        d="m11 10.9 5 5 5-5"
        fill="none"
        stroke={`url(#${arrow})`}
        strokeWidth="3.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M5 19.4h5.8l1.6 3h7.2l1.6-3H27v6.2a3.4 3.4 0 0 1-3.4 3.4H8.4A3.4 3.4 0 0 1 5 25.6z"
        fill={`url(#${tray})`}
        stroke="var(--icon-silver-deep)"
        strokeOpacity=".6"
        strokeWidth=".9"
      />
      <path
        d="M6.2 19.4h19.6"
        stroke="#fff"
        strokeOpacity=".4"
        strokeWidth="1"
        strokeLinecap="round"
      />
    </svg>
  );
}
