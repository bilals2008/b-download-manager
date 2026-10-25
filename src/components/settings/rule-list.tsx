import { useRef, useState } from "react";

import { CloseGlyph } from "@/components/icons/ui-icons";
import { SettingsButton } from "@/components/settings/settings-button";
import { FIELD } from "@/components/ui/field";

interface RuleListProps {
  id: string;
  label: string;
  placeholder: string;
  values: string[];
  onChange: (values: string[]) => void;
  /** Turn whatever was typed into the stored form, or "" when it is junk. */
  clean: (value: string) => string;
  /** How one stored entry is shown, when it is not stored that way. */
  format?: (value: string) => string;
  emptyText: string;
  invalidText: string;
  max: number;
}

/**
 * A list of short rules with a field under it. Each entry is removed by its own
 * button, and an entry the cleaner refuses is explained instead of stored.
 */
export function RuleList({
  id,
  label,
  placeholder,
  values,
  onChange,
  clean,
  format = (value) => value,
  emptyText,
  invalidText,
  max,
}: RuleListProps) {
  const [draft, setDraft] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const full = values.length >= max;

  function add() {
    if (full) {
      setProblem(`The list holds at most ${max} entries.`);
      return;
    }
    const entry = clean(draft);
    if (!entry) {
      setProblem(invalidText);
      return;
    }
    if (values.includes(entry)) {
      setProblem("That entry is already in the list.");
      return;
    }
    onChange([...values, entry]);
    setDraft("");
    setProblem(null);
    field.current?.focus();
  }

  function remove(value: string) {
    onChange(values.filter((entry) => entry !== value));
    setProblem(null);
    field.current?.focus();
  }

  return (
    <div>
      {values.length ? (
        <ul aria-label={`${label} list`} className="flex flex-wrap gap-1.5 pb-2">
          {values.map((value) => (
            <li
              key={value}
              className="inline-flex items-center gap-1 rounded-chrome border border-chrome-line bg-chrome-canvas py-0.5 pr-0.5 pl-2 text-chrome-micro text-chrome-menu-text"
            >
              <span className="max-w-64 truncate font-mono">
                {format(value)}
              </span>
              <button
                type="button"
                aria-label={`Remove ${format(value)}`}
                onClick={() => remove(value)}
                className="flex size-5 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
              >
                <CloseGlyph className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="pb-2 text-chrome-micro text-chrome-muted">{emptyText}</p>
      )}

      <form
        className="flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          add();
        }}
      >
        <input
          id={id}
          ref={field}
          className={FIELD}
          value={draft}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={problem !== null || undefined}
          aria-describedby={problem ? `${id}-problem` : undefined}
          onChange={(event) => {
            setDraft(event.target.value);
            setProblem(null);
          }}
        />
        <SettingsButton type="submit" disabled={full}>
          Add
        </SettingsButton>
      </form>

      {problem ? (
        <p
          id={`${id}-problem`}
          role="alert"
          className="mt-1.5 text-chrome-micro text-chrome-error"
        >
          {problem}
        </p>
      ) : null}
    </div>
  );
}
