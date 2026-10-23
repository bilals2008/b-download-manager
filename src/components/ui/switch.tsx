import { cn } from "@/lib/utils";

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** The name a screen reader announces; the visible name sits beside it. */
  label: string;
  disabled?: boolean;
  className?: string;
}

/**
 * An on/off switch for a preference. A preference that takes effect the moment
 * it is flipped reads as a switch, and the switch is the one control a desktop
 * user recognises without reading the label: the knob and the track carry the
 * state, so a page of them can be read from the right edge alone.
 */
export function Switch({
  checked,
  onChange,
  label,
  disabled,
  className,
}: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "inline-flex h-5 w-9 shrink-0 items-center rounded-full border p-0.5 transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-offset-2 focus-visible:ring-offset-chrome-panel focus-visible:outline-none",
        checked
          ? "border-primary bg-primary"
          : "border-chrome-line bg-chrome-canvas",
        disabled
          ? "cursor-default opacity-45"
          : checked
            ? "cursor-pointer"
            : "cursor-pointer hover:border-chrome-muted",
        className,
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "size-4 rounded-full transition-transform duration-150 ease-out motion-reduce:transition-none",
          checked
            ? "translate-x-3.5 bg-primary-foreground"
            : "translate-x-0 bg-chrome-muted",
        )}
      />
    </button>
  );
}
