import { releaseNoteLines } from "@/lib/updates";
import { cn } from "@/lib/utils";

interface ReleaseNotesProps {
  notes: string;
  className?: string;
}

/**
 * What a release says, as text.
 *
 * The body arrives as markdown; this draws the words in the shapes the reader
 * expects - a heading where the release had one, a bullet where it had a dash -
 * instead of showing the marks themselves.
 */
export function ReleaseNotes({ notes, className }: ReleaseNotesProps) {
  return (
    <div
      className={cn(
        "flex flex-col gap-1 text-xs leading-relaxed text-chrome-menu-text",
        className,
      )}
    >
      {releaseNoteLines(notes).map((line, index) =>
        line.kind === "bullet" ? (
          <p key={index} className="flex gap-2 pl-1">
            <span aria-hidden="true" className="text-chrome-muted">
              &bull;
            </span>
            <span className="min-w-0">{line.text}</span>
          </p>
        ) : (
          <p
            key={index}
            className={cn(
              line.kind === "heading" &&
                "mt-2 font-medium text-chrome-title-text first:mt-0",
            )}
          >
            {line.text}
          </p>
        ),
      )}
    </div>
  );
}
