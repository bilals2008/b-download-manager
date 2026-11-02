import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/**
 * The frame of one page of the settings window. Every page opens with the same
 * heading block and then a run of sections, so a page reads as one document
 * instead of a stack of cards.
 */
export function SettingsPage({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="flex w-full flex-col px-5">
      <header className="pb-3">
        <h2 className="text-sm font-medium text-chrome-title-text">{title}</h2>
      </header>
      <div className="flex flex-col">{children}</div>
    </div>
  );
}

/**
 * A titled group of settings. The caption is the only thing that separates one
 * group from the next: no boxes, no rules, no extra spacing between rows.
 */
export function Section({
  id,
  title,
  action,
  children,
}: {
  /** The anchor a search result scrolls to, when it opens the whole group. */
  id?: string;
  title: string;
  /** A control for the group as a whole, beside its caption. */
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section
      id={id}
      tabIndex={id ? -1 : undefined}
      data-setting-section={id ? "" : undefined}
      aria-label={title}
      className="flex flex-col focus:outline-none"
    >
      <div className="flex items-center gap-3 pb-1">
        <h3 className="text-xs font-medium text-chrome-muted">{title}</h3>
        {action}
      </div>
      <div className="flex flex-col">{children}</div>
    </section>
  );
}

/**
 * One setting: the label on the left, the control that changes it on the
 * right. This is the row every desktop preferences window is made of, and it
 * keeps a page scannable down the left edge.
 */
export function SettingRow({
  label,
  htmlFor,
  settingId,
  stacked,
  children,
}: {
  label: string;
  /** Set when the control is a plain input, so the name becomes its label. */
  htmlFor?: string;
  /** The anchor a settings search result scrolls to and highlights. */
  settingId?: string;
  /** The control sits under the name: for a path or an address, which is wide. */
  stacked?: boolean;
  children: ReactNode;
}) {
  const name = "block text-xs leading-5 text-chrome-title-text";

  return (
    <div
      id={settingId}
      tabIndex={settingId ? -1 : undefined}
      data-setting-row={settingId ? "" : undefined}
      className={cn(
        "gap-4 py-2 focus:outline-none",
        stacked ? "flex flex-col" : "flex items-center justify-between",
      )}
    >
      <div className="min-w-0">
        {htmlFor ? (
          <label htmlFor={htmlFor} className={name}>
            {label}
          </label>
        ) : (
          <span className={name}>{label}</span>
        )}
      </div>
      <div className={stacked ? "w-full" : "shrink-0"}>{children}</div>
    </div>
  );
}
