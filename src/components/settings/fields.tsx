import type { ReactNode } from "react";

/** The box every field in the options window uses. */
export const FIELD =
  "w-full rounded-chrome border border-chrome-line bg-chrome-canvas px-2 py-1.5 text-xs text-chrome-menu-text placeholder:text-chrome-muted/70 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none";

/** The label above a field. */
export const LABEL = "mb-1 block text-xs text-chrome-muted";

/** The line of help under a field. */
export const HINT = "mt-1 text-chrome-micro text-chrome-muted";

/** A label, a control and the help under it. */
export function Field({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor?: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div>
      <label htmlFor={htmlFor} className={LABEL}>
        {label}
      </label>
      {children}
      {hint ? <p className={HINT}>{hint}</p> : null}
    </div>
  );
}

/** A one line text field. */
export function TextInput({
  id,
  value,
  onChange,
  placeholder,
}: {
  id: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  return (
    <input
      id={id}
      className={FIELD}
      value={value}
      placeholder={placeholder}
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
}: {
  id: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  suffix?: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <input
        id={id}
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        className={`${FIELD} w-28 tabular-nums`}
        value={value}
        onChange={(event) => {
          const next = Number.parseInt(event.target.value, 10);
          if (Number.isFinite(next)) {
            onChange(Math.min(max, Math.max(min, next)));
          }
        }}
      />
      {suffix ? <span className="text-xs text-chrome-muted">{suffix}</span> : null}
    </div>
  );
}
