import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/**
 * The frame of one page of the settings window. Every page opens with the same
 * heading block and then a run of sections, so a page reads as one document
 * instead of a stack of cards.
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
    <div className="flex w-full flex-col px-5 py-5">
      <header className="pb-5">
        <h2 className="text-sm font-medium text-chrome-title-text">{title}</h2>
        <p className="mt-0.5 text-xs text-chrome-muted">{description}</p>
      </header>
      <div className="flex flex-col gap-6">{children}</div>
    </div>
  );
}

/**
 * A titled group of settings. The caption and the space above it are the only
 * things that separate one group from the next: no boxes, no rules.
 */
export function Section({
  title,
  action,
  children,
}: {
  title: string;
  /** A control for the group as a whole, beside its caption. */
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section aria-label={title} className="flex flex-col">
      <div className="flex items-center justify-between gap-3 pb-1.5">
        <h3 className="text-xs font-medium text-chrome-muted">{title}</h3>
        {action}
      </div>
      <div className="flex flex-col">{children}</div>
    </section>
  );
}

/**
 * One setting: what it is on the left, the control that changes it on the
 * right, and the explanation under the name. This is the row every desktop
 * preferences window is made of, and it keeps a page scannable down the left
 * edge instead of making every control carry its own caption.
 */
export function SettingRow({
  label,
  hint,
  htmlFor,
  stacked,
  children,
}: {
  label: string;
  hint?: ReactNode;
  /** Set when the control is a plain input, so the name becomes its label. */
  htmlFor?: string;
  /** The control sits under the name: for a path or an address, which is wide. */
  stacked?: boolean;
  children: ReactNode;
}) {
  const name = "block text-xs leading-5 text-chrome-title-text";

  return (
    <div
      className={cn(
        "gap-4 py-2",
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
        {hint ? (
          <p className="text-chrome-micro leading-4 text-chrome-muted">{hint}</p>
        ) : null}
      </div>
      <div className={stacked ? "w-full" : "shrink-0"}>{children}</div>
    </div>
  );
}
