import type { ButtonHTMLAttributes } from "react";

import { cn } from "@/lib/utils";

interface SettingsButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Destructive actions are the only ones that carry a filled colour. */
  tone?: "normal" | "danger";
}

/** A small button inside a settings page, matching the dialog's footer. */
export function SettingsButton({
  tone = "normal",
  className,
  type = "button",
  ...props
}: SettingsButtonProps) {
  return (
    <button
      {...props}
      type={type}
      className={cn(
        "inline-flex shrink-0 items-center justify-center gap-1.5 rounded-chrome border px-2.5 py-1.5 text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-45",
        tone === "danger"
          ? "border-chrome-danger bg-chrome-danger text-chrome-danger-text hover:opacity-90"
          : "border-chrome-line bg-chrome-canvas text-chrome-menu-text hover:border-chrome-muted",
        className,
      )}
    />
  );
}
