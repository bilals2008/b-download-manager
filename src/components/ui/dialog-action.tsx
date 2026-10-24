import type { ButtonHTMLAttributes, ReactNode } from "react";

import { cn } from "@/lib/utils";

type DialogActionTone = "primary" | "secondary" | "danger" | "quiet";

const TONES: Record<DialogActionTone, string> = {
  primary: "bg-primary text-primary-foreground hover:opacity-90",
  secondary: "bg-chrome-panel text-chrome-menu-text hover:bg-chrome-menu-hover",
  // A destructive answer is named by its word, in the error tone, instead of
  // being painted as a red slab next to the safe one.
  danger: "bg-chrome-panel text-chrome-error hover:bg-chrome-menu-hover",
  quiet: "text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text",
};

interface DialogActionProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: DialogActionTone;
  icon?: ReactNode;
}

/**
 * A button of a small dialog: 28px tall, one accent at a time, and a dimmed
 * but readable command when it cannot act. Shared by the File Exists and the
 * Download Failed windows so the two read as the same family.
 */
export function DialogAction({
  tone = "secondary",
  icon,
  className,
  children,
  ...props
}: DialogActionProps) {
  return (
    <button
      type="button"
      {...props}
      className={cn(
        "inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-chrome px-3 text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-45",
        TONES[tone],
        className,
      )}
    >
      {icon}
      {children}
    </button>
  );
}
