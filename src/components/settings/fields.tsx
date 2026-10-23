import { FIELD } from "@/components/ui/field";

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
}: {
  id: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  /** Addresses and paths are read one character at a time. */
  mono?: boolean;
  disabled?: boolean;
}) {
  return (
    <input
      id={id}
      className={mono ? `${FIELD} font-mono` : FIELD}
      value={value}
      placeholder={placeholder}
      disabled={disabled}
      spellCheck={false}
      onChange={(event) => onChange(event.target.value)}
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
  return (
    <div className="flex items-center gap-2">
      <input
        id={id}
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        disabled={disabled}
        className={`${FIELD} w-24 text-right tabular-nums`}
        value={value}
        onChange={(event) => {
          const next = Number.parseInt(event.target.value, 10);
          if (Number.isFinite(next)) {
            onChange(Math.min(max, Math.max(min, next)));
          }
        }}
      />
      {suffix ? (
        <span
          className={
            disabled
              ? "text-xs text-chrome-muted/60"
              : "text-xs text-chrome-muted"
          }
        >
          {suffix}
        </span>
      ) : null}
    </div>
  );
}
