import type { ButtonHTMLAttributes, ReactNode } from "react";

import { cn } from "@/lib/utils";

type Tone = "primary" | "secondary" | "danger" | "quiet";

const TONES: Record<Tone, string> = {
  primary: "bg-primary text-primary-foreground hover:opacity-90",
  secondary: "bg-chrome-panel text-chrome-menu-text hover:bg-chrome-menu-hover",
  // A destructive command is marked by its word and its mark, not by a red
  // slab: the window keeps one action in the accent at a time.
  danger: "bg-chrome-panel text-chrome-error hover:bg-chrome-menu-hover",
  quiet: "text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text",
};

interface ActionButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: Tone;
  icon?: ReactNode;
}

/**
 * The one button of the progress window: 28px tall, a hairline-free surface,
 * and the accent only for the action the moment is waiting for. A command
 * that cannot act is dimmed but still readable, as desktop controls are.
 */
export function ActionButton({
  tone = "secondary",
  icon,
  className,
  children,
  ...props
}: ActionButtonProps) {
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
