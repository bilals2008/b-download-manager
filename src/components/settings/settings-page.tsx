import type { ReactNode } from "react";

import { Badge } from "@/components/ui/badge";

/**
 * The shell of a page in the options window: a heading, a line of
 * explanation, and the blocks the page is made of. Every page uses it, so
 * they all read the same way.
 */
export function SettingsPage({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div className="mx-auto flex max-w-[42rem] flex-col px-5 py-6 sm:px-6">
      <header>
        <h2 className="text-base font-medium text-chrome-title-text">{title}</h2>
        <p className="mt-1 text-xs text-chrome-muted">{description}</p>
      </header>
      <div className="mt-5 flex flex-col gap-1">{children}</div>
    </div>
  );
}

/** One titled block of a page. */
export function Section({
  title,
  planned,
  children,
}: {
  title: string;
  /** Marks a block the build does not have yet. */
  planned?: boolean;
  children: ReactNode;
}) {
  return (
    <section aria-label={title} className="py-5 first:pt-0 last:pb-0">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-xs font-medium text-chrome-title-text">{title}</h3>
        {planned ? (
          <Badge
            variant="secondary"
            className="px-1.5 py-0 text-chrome-micro font-normal text-chrome-muted"
          >
            Planned
          </Badge>
        ) : null}
      </div>
      <div className="mt-3">{children}</div>
    </section>
  );
}
