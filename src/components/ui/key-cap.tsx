import { cn } from "@/lib/utils";

/**
 * A keyboard key, drawn as the cap the tips dialog prints. One component so
 * the caps cannot drift apart.
 */
export function KeyCap({
  children,
  className,
}: {
  children: string;
  className?: string;
}) {
  return (
    <kbd
      className={cn(
        "rounded-sm bg-chrome-menu-hover px-1.5 py-px font-mono text-chrome-micro leading-4 text-chrome-menu-text",
        className,
      )}
    >
      {children}
    </kbd>
  );
}
