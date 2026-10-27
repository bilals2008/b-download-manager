import avatar from "@/assets/brand/github-avatar.png";
import { Tooltip } from "@/components/ui/tooltip";
import { AUTHOR } from "@/lib/brand";
import { openExternal } from "@/lib/browser";
import { cn } from "@/lib/utils";

/**
 * The author's GitHub avatar, as the link to their profile.
 *
 * A face instead of a sentence: both About surfaces already name the build, so
 * the signature is the one mark that says who wrote it without a line the
 * reader has to skip past. The address stays a real link, so a screen reader
 * announces it as a link and the browser shows it in the status bar, but the
 * click goes to the system: a webview that followed it would lose the page it
 * was on.
 */
export function AuthorAvatar({ className }: { className?: string }) {
  const label = `${AUTHOR.name} on GitHub`;

  return (
    <Tooltip content={label} side="left" className={cn("shrink-0", className)}>
      <a
        href={AUTHOR.github}
        aria-label={label}
        onClick={(event) => {
          event.preventDefault();
          openExternal(AUTHOR.github);
        }}
        className="size-7 rounded-full border border-chrome-line ring-offset-1 ring-offset-chrome-canvas transition-colors hover:border-chrome-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-chrome-focus"
      >
        <img
          src={avatar}
          alt=""
          draggable={false}
          className="size-full rounded-full"
        />
      </a>
    </Tooltip>
  );
}
