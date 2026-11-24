import { flagUrl } from "@/lib/flags";
import { cn } from "@/lib/utils";

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
  const url = flagUrl(code);
  if (!url) {
    return null;
  }
  return (
    <img
      src={url}
      alt=""
      aria-hidden
      draggable={false}
      className={cn(
        "h-3 w-4 shrink-0 rounded-[2px] object-cover ring-1 ring-chrome-line ring-inset",
        className,
      )}
    />
  );
}
