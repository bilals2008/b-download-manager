import { cn } from "@/lib/utils";

/**
 * A keyboard key, drawn as the cap the tooltips and the tips dialog print.
 * One component so the caps cannot drift apart.
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
        "rounded-sm border border-chrome-line bg-chrome-toolbar-active px-1.5 py-0.5 font-mono text-chrome-micro text-chrome-muted",
        className,
      )}
    >
      {children}
    </kbd>
  );
}
