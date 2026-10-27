import { openExternal } from "@/lib/browser";
import { cn } from "@/lib/utils";

/** Who wrote this build and where their work can be read about. */
export const AUTHOR = {
  name: "Muhammad Bilal Hassan",
  site: "https://mbilalhassan.vercel.app/",
} as const;

/**
 * The author line both About surfaces end with.
 *
 * Small and to one side: it is a signature, not a setting. The address is a
 * real link, so the browser shows it in the status bar and a screen reader
 * announces it as a link, but the click goes to the system: a webview that
 * followed it would lose the page it was on.
 */
export function AuthorLink({ className }: { className?: string }) {
  return (
    <p
      className={cn(
        "flex items-baseline justify-between gap-3 text-chrome-caption text-chrome-muted",
        className,
      )}
    >
      <span>Built by</span>
      <a
        href={AUTHOR.site}
        onClick={(event) => {
          event.preventDefault();
          openExternal(AUTHOR.site);
        }}
        className="text-chrome-focus underline decoration-chrome-line underline-offset-2 hover:decoration-chrome-focus"
      >
        {AUTHOR.name}
      </a>
    </p>
  );
}
