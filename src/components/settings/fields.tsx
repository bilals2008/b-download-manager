import { useState, type KeyboardEvent } from "react";

import { FIELD } from "@/components/ui/field";
import { cn } from "@/lib/utils";

/** The box every text field in the application uses. */
export { FIELD };

/** A one line text field. */
export function TextInput({
  id,
  value,
  onChange,
  placeholder,
  mono,
  disabled,
  invalid,
  onBlur,
  onKeyDown,
}: {
  id: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  /** Addresses and paths are read one character at a time. */
  mono?: boolean;
  disabled?: boolean;
  invalid?: boolean;
  onBlur?: () => void;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
}) {
  return (
    <input
      id={id}
      className={cn(
        FIELD,
        mono && "font-mono",
        invalid && "border-chrome-danger hover:border-chrome-danger",
      )}
      value={value}
      placeholder={placeholder}
      disabled={disabled}
      spellCheck={false}
      aria-invalid={invalid || undefined}
      onChange={(event) => onChange(event.target.value)}
      onBlur={onBlur}
      onKeyDown={onKeyDown}
    />
  );
}

/** A field that only takes a number, with the unit written next to it. */
export function NumberInput({
  id,
  value,
  onChange,
  min = 1,
  max = 999,
  suffix,
  disabled,
}: {
  id: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  suffix?: string;
  disabled?: boolean;
}) {
  // The field keeps what was typed so it can be cleared and retyped; the
  // value only goes up when it is a number in range, and the clamp happens on
  // leaving the field.
  const [draft, setDraft] = useState(() => String(value));
  const [shown, setShown] = useState(value);

  if (shown !== value) {
    setShown(value);
    setDraft(String(value));
  }

  const parsed = draft.trim() === "" ? Number.NaN : Number(draft);
  const invalid =
    draft.trim() !== "" && (!Number.isFinite(parsed) || parsed < min || parsed > max);

  function commit() {
    if (!Number.isFinite(parsed)) {
      setDraft(String(value));
      return;
    }
    const clamped = Math.min(max, Math.max(min, Math.round(parsed)));
    setDraft(String(clamped));
    if (clamped !== value) {
      onChange(clamped);
    }
  }

  return (
    <div className="flex items-center gap-2">
      <input
        id={id}
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        disabled={disabled}
        className={cn(
          FIELD,
          "w-24 text-right tabular-nums",
          invalid && "border-chrome-danger hover:border-chrome-danger",
        )}
        value={draft}
        aria-invalid={invalid || undefined}
        onChange={(event) => {
          const raw = event.target.value;
          setDraft(raw);
          const next = Number(raw);
          if (
            raw.trim() !== "" &&
            Number.isInteger(next) &&
            next >= min &&
            next <= max
          ) {
            onChange(next);
          }
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.currentTarget.blur();
          }
        }}
      />
      {suffix ? (
        <span
          className={
            disabled ? "text-xs text-chrome-muted/60" : "text-xs text-chrome-muted"
          }
        >
          {suffix}
        </span>
      ) : null}
    </div>
  );
}
