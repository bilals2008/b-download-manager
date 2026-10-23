import type { ReactNode } from "react";

/**
 * The small chrome glyphs: controls, menu ticks, chevrons and the settings
 * navigation. Drawn here so the app owns every mark it shows, in one stroke
 * weight, and so no third-party icon set decides how the window looks.
 */

interface GlyphProps {
  className?: string;
}

export function Glyph({
  className,
  children,
  width = 1.5,
}: GlyphProps & { children: ReactNode; width?: number }) {
  return (
    <svg
      viewBox="0 0 16 16"
      aria-hidden="true"
      focusable="false"
      className={className}
      fill="none"
      stroke="currentColor"
      strokeWidth={width}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </svg>
  );
}

export function ChevronDown({ className }: GlyphProps) {
  return (
    <Glyph className={className}>
      <path d="m4 6 4 4 4-4" />
    </Glyph>
  );
}

export function ChevronUp({ className }: GlyphProps) {
  return (
    <Glyph className={className}>
      <path d="m4 10 4-4 4 4" />
    </Glyph>
  );
}

export function ChevronRight({ className }: GlyphProps) {
  return (
    <Glyph className={className}>
      <path d="m6 3.5 4.5 4.5L6 12.5" />
    </Glyph>
  );
}

/**
 * The tree disclosure chevron: wider, rounder and a step heavier than the menu
 * arrows, so opening a category reads as a control and not as decoration.
 */
export function DisclosureGlyph({ className }: GlyphProps) {
  return (
    <Glyph className={className} width={2}>
      <path d="m6.1 4.1 3.9 3.9-3.9 3.9" />
    </Glyph>
  );
}

export function ChevronsUpDown({ className }: GlyphProps) {
  return (
    <Glyph className={className} width={1.3}>
      <path d="m4.5 6.5 3.5-3.5 3.5 3.5" />
      <path d="m4.5 9.5 3.5 3.5 3.5-3.5" />
    </Glyph>
  );
}

export function CheckGlyph({ className }: GlyphProps) {
  return (
    <Glyph className={className} width={1.8}>
      <path d="m3.2 8.4 3.2 3.2 6.4-7" />
    </Glyph>
  );
}

export function CloseGlyph({ className }: GlyphProps) {
  return (
    <Glyph className={className}>
      <path d="M4.2 4.2l7.6 7.6M11.8 4.2l-7.6 7.6" />
    </Glyph>
  );
}

export function SearchGlyph({ className }: GlyphProps) {
  return (
    <Glyph className={className}>
      <circle cx="7" cy="7" r="4.2" />
      <path d="m10.2 10.2 3.1 3.1" />
    </Glyph>
  );
}

export function PlusGlyph({ className }: GlyphProps) {
  return (
    <Glyph className={className} width={1.7}>
      <path d="M8 3.2v9.6M3.2 8h9.6" />
    </Glyph>
  );
}

export function DownloadGlyph({ className }: GlyphProps) {
  return (
    <Glyph className={className} width={1.7}>
      <path d="M8 2.8v7.6" />
      <path d="m4.4 7.2 3.6 3.6 3.6-3.6" />
      <path d="M2.8 12.4v.8h10.4v-.8" />
    </Glyph>
  );
}

/**
 * A window dropping into the tray.
 *
 * The window outline is what tells this apart from a download button: the
 * arrow leaves a window, it does not fetch a file. The line at the foot is the
 * tray the window goes into.
 */
export function MinimizeToTrayGlyph({ className }: GlyphProps) {
  return (
    <Glyph className={className}>
      <rect x="3.2" y="2.4" width="9.6" height="5.8" rx="1.3" />
      <path d="M8 9v3.2" />
      <path d="m6.3 10.6 1.7 1.7 1.7-1.7" />
      <path d="M3 14.8h10" />
    </Glyph>
  );
}

export function PanelGlyph({
  collapsed,
  className,
}: GlyphProps & { collapsed: boolean }) {
  return (
    <Glyph className={className} width={1.6}>
      <rect x="2.3" y="3.4" width="11.4" height="9.2" rx="2.4" />
      <path d="M6.2 3.7v8.6" />
      <path
        d={collapsed ? "m8.9 6.5 1.5 1.5-1.5 1.5" : "M10.4 6.5 8.9 8l1.5 1.5"}
      />
    </Glyph>
  );
}

export function SpinnerGlyph({ className }: GlyphProps) {
  return (
    <Glyph className={className} width={1.8}>
      <path d="M8 2.2a5.8 5.8 0 1 1-5.8 5.8" />
    </Glyph>
  );
}

/** The settings pages. One glyph per page, all at the same weight. */
export type SettingsGlyphKind =
  | "general"
  | "file-types"
  | "save-to"
  | "downloads"
  | "connection"
  | "proxy"
  | "sites"
  | "dial-up"
  | "sounds";

export function SettingsGlyph({
  kind,
  className,
}: GlyphProps & { kind: SettingsGlyphKind }) {
  switch (kind) {
    case "file-types":
      return (
        <Glyph className={className}>
          <path d="M4.6 1.9h3.9l2.9 2.9v9.3H4.6z" />
          <path d="M8.5 2v2.9h2.9" />
        </Glyph>
      );
    case "save-to":
      return (
        <Glyph className={className}>
          <path d="M2.2 5.6A1.6 1.6 0 0 1 3.8 4h3l1.3 1.5h4.1a1.6 1.6 0 0 1 1.6 1.6v5.5a1.6 1.6 0 0 1-1.6 1.6H3.8a1.6 1.6 0 0 1-1.6-1.6z" />
          <path d="M8 7.4v3.9M6.3 9.7 8 11.4l1.7-1.7" />
        </Glyph>
      );
    case "downloads":
      return <DownloadGlyph className={className} />;
    case "connection":
      return (
        <Glyph className={className}>
          <path d="M4.6 5.2h6.8M4.6 10.8h6.8" />
          <path d="M2.6 3.6v3.2M13.4 9.2v3.2" />
          <circle cx="12" cy="5.2" r="1.5" />
          <circle cx="4" cy="10.8" r="1.5" />
        </Glyph>
      );
    case "proxy":
      return (
        <Glyph className={className}>
          <path d="M8 1.8 13 4v4a6 6 0 0 1-5 6 6 6 0 0 1-5-6V4z" />
          <path d="m6 8 1.5 1.5L10.2 6.6" />
        </Glyph>
      );
    case "sites":
      return (
        <Glyph className={className}>
          <circle cx="5.6" cy="5.4" r="2.1" />
          <path d="M2.2 13.2c0-2 1.5-3.3 3.4-3.3s3.4 1.3 3.4 3.3" />
          <path d="M10.4 6.6h3.4M10.4 9.4h3.4M10.4 12.2h2.2" />
        </Glyph>
      );
    case "dial-up":
      return (
        <Glyph className={className}>
          <path d="M4.2 2.4h2.6l1 2.7-1.5 1.2a9.4 9.4 0 0 0 3.4 3.4l1.2-1.5 2.7 1v2.6a1.4 1.4 0 0 1-1.6 1.4A11.4 11.4 0 0 1 2.8 4a1.4 1.4 0 0 1 1.4-1.6z" />
        </Glyph>
      );
    case "sounds":
      return (
        <Glyph className={className}>
          <path d="M2.4 6.2h2.4L8 3.4v9.2L4.8 9.8H2.4z" />
          <path d="M10.4 6a3 3 0 0 1 0 4M12.2 4a5.6 5.6 0 0 1 0 8" />
        </Glyph>
      );
    default:
      return (
        <Glyph className={className}>
          <path d="M2.4 5h11.2M2.4 8h11.2M2.4 11h11.2" />
          <circle cx="5.6" cy="5" r="1.5" fill="currentColor" stroke="none" />
          <circle cx="10" cy="8" r="1.5" fill="currentColor" stroke="none" />
          <circle cx="6.8" cy="11" r="1.5" fill="currentColor" stroke="none" />
        </Glyph>
      );
  }
}
