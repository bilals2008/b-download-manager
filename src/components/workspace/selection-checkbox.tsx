import { useEffect, useRef } from "react";

import { CheckGlyph } from "@/components/icons/ui-icons";
import { cn } from "@/lib/utils";

interface SelectionCheckboxProps {
  checked: boolean;
  /** Some of the rows are picked, not all of them. */
  indeterminate?: boolean;
  label: string;
  onToggle: () => void;
  className?: string;
}

/**
 * The compact checkbox that picks rows, in the name column and in its header.
 * The native input keeps the keyboard and screen-reader behaviour; only its
 * appearance is replaced, so Shift+click and Space work the way they should.
 */
export function SelectionCheckbox({
  checked,
  indeterminate = false,
  label,
  onToggle,
  className,
}: SelectionCheckboxProps) {
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (ref.current) {
      ref.current.indeterminate = indeterminate;
    }
  }, [indeterminate]);

  return (
    <span
      className={cn(
        "relative flex size-4 shrink-0 items-center justify-center",
        className,
      )}
    >
      <input
        ref={ref}
        type="checkbox"
        checked={checked}
        aria-label={label}
        onClick={(event) => event.stopPropagation()}
        onChange={onToggle}
        className={cn(
          "peer size-4 appearance-none rounded-sm border border-input bg-chrome-canvas transition-colors hover:border-chrome-muted checked:border-primary checked:bg-primary focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-offset-1 focus-visible:ring-offset-chrome-canvas focus-visible:outline-none",
          indeterminate && "border-primary bg-primary",
        )}
      />
      <CheckGlyph className="pointer-events-none absolute size-3 text-primary-foreground opacity-0 peer-checked:opacity-100" />
      {indeterminate ? (
        <span className="pointer-events-none absolute h-0.5 w-2 rounded-full bg-primary-foreground" />
      ) : null}
    </span>
  );
}
