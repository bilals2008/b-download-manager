import { FLAG_ATLAS, flagCell } from "@/lib/flags";
import { cn } from "@/lib/utils";

/**
 * How much smaller the flag is drawn than its cell: 32 by 24 in the picture,
 * 16 by 12 on screen, so a screen with more pixels than points still has its
 * own for the flag.
 */
const DRAWN_SCALE = 0.5;

/**
 * The flag of the country a download's server answers from.
 *
 * Sixteen by twelve pixels — the shape the server row draws beside the country
 * name — with a hairline, so a flag that is mostly white still has an edge on
 * the dark surface. It is decoration: the country is named in words beside it,
 * so it carries no text of its own.
 */
export function CountryFlag({
  code,
  className,
}: {
  code: string | null | undefined;
  className?: string;
}) {
  const cell = flagCell(code);
  if (!cell) {
    return null;
  }
  const { url, columns, rows, cellWidth, cellHeight } = FLAG_ATLAS;
  return (
    <span
      aria-hidden
      className={cn(
        "h-3 w-4 shrink-0 rounded-[2px] bg-no-repeat ring-1 ring-chrome-line ring-inset",
        className,
      )}
      style={{
        backgroundImage: `url("${url}")`,
        backgroundSize: `${columns * cellWidth * DRAWN_SCALE}px ${rows * cellHeight * DRAWN_SCALE}px`,
        backgroundPosition: `${-cell.x * DRAWN_SCALE}px ${-cell.y * DRAWN_SCALE}px`,
      }}
    />
  );
}
