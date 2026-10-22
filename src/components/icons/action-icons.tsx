import { Glyph } from "@/components/icons/ui-icons";

/**
 * The glyphs of a command: the ones a menu, a toolbar or a button needs to
 * name an action without a word. They share the stroke of the chrome glyphs,
 * so a menu entry and the button that does the same thing look like one set.
 */

interface ActionGlyphProps {
  className?: string;
}

export function PlayGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <path d="M5.5 3.5 12 8l-6.5 4.5z" />
    </Glyph>
  );
}

export function PauseGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <path d="M6 3.5v9M10 3.5v9" />
    </Glyph>
  );
}

export function StopGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <rect x="4.5" y="4.5" width="7" height="7" rx="1" />
    </Glyph>
  );
}

export function RefreshGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <path d="M13 8a5 5 0 1 1-1.46-3.54" />
      <path d="M13 2.5V5h-2.5" />
    </Glyph>
  );
}

export function TrashGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <path d="M3 4.5h10" />
      <path d="M6.5 4.5V3h3v1.5" />
      <path d="m4.8 4.5.6 8.5h5.2l.6-8.5" />
    </Glyph>
  );
}

export function OpenFileGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <path d="M9.5 2.5h4v4" />
      <path d="m13.5 2.5-5 5" />
      <path d="M12 9.5v3.5H2.5V3.5H6" />
    </Glyph>
  );
}

export function FolderGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <path d="M2 4.5h4.2l1.4 1.8H14v6.2H2z" />
    </Glyph>
  );
}

export function CopyGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <rect x="5.5" y="5.5" width="8" height="8" rx="1" />
      <path d="M10.5 5.5v-2h-8v8h2" />
    </Glyph>
  );
}

export function InfoGlyph({ className }: ActionGlyphProps) {
  return (
    <Glyph className={className}>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 7.5v3.5M8 5.3v.2" />
    </Glyph>
  );
}
