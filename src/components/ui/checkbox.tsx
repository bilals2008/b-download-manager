import { useId, type ComponentProps } from "react";
import { Check } from "lucide-react";

import { cn } from "@/lib/utils";

interface CheckboxProps extends Omit<ComponentProps<"input">, "type"> {
  label: string;
  hint?: string;
}

/**
 * A themed checkbox. The native input keeps the keyboard and screen-reader
 * behaviour; only its appearance is replaced, so no ARIA widget is invented.
 */
export function Checkbox({
  label,
  hint,
  className,
  disabled,
  ...props
}: CheckboxProps) {
  const generatedId = useId();
  const { id: providedId, ...inputProps } = props;
  const id = providedId ?? generatedId;

  return (
    <label
      htmlFor={id}
      className={cn(
        "flex items-start gap-2.5 py-0.5",
        disabled ? "cursor-default opacity-55" : "cursor-pointer",
        className,
      )}
    >
      <span className="relative mt-0.5 flex size-4 shrink-0 items-center justify-center">
        <input
          id={id}
          type="checkbox"
          disabled={disabled}
          className="peer size-4 appearance-none rounded-chrome border border-input bg-chrome-canvas transition-colors hover:border-chrome-muted checked:border-primary checked:bg-primary disabled:border-chrome-line disabled:bg-chrome-panel disabled:checked:bg-chrome-line/60"
          {...inputProps}
        />
        <Check
          aria-hidden="true"
          strokeWidth={3}
          className="pointer-events-none absolute size-3 text-primary-foreground opacity-0 peer-checked:opacity-100 peer-disabled:text-chrome-muted"
        />
      </span>
      <span className="min-w-0">
        <span
          className={cn(
            "block text-xs leading-5",
            disabled ? "text-chrome-muted" : "text-chrome-menu-text",
          )}
        >
          {label}
        </span>
        {hint ? (
          <span className="block text-xs text-chrome-muted">{hint}</span>
        ) : null}
      </span>
    </label>
  );
}
