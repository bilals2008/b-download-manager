import { useRef, useState } from "react";

import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { SettingsButton } from "@/components/settings/settings-button";
import { Tooltip } from "@/components/ui/tooltip";
import { broadcastSettings } from "@/lib/app-events";
import {
  SHORTCUT_DEFINITIONS,
  FIXED_SHORTCUT_ROWS,
  bindingIsDefault,
  formatBinding,
  getEffectiveBinding,
  comboFromEvent,
  resetAllShortcutBindings,
  resetShortcutBinding,
  setShortcutBinding,
  shortcutsAreCustomized,
  type ShortcutDefinition,
} from "@/lib/shortcuts";

/**
 * Keyboard shortcuts: every command's current key, and the recorder that
 * changes it. The page is a reader of the registry in `lib/shortcuts.ts` —
 * it owns no list of its own — so the menus, the tooltips and the dispatcher
 * agree with what is shown here, and a change is on disk the moment it is
 * made.
 */
export function ShortcutsSettings() {
  // Re-render the rows after any write; the bindings themselves come from the
  // registry, so the page needs no copy of them.
  const [, setRevision] = useState(0);
  const bump = () => {
    setRevision((value) => value + 1);
    // Other windows read the same file; this tells them to look again.
    broadcastSettings();
  };

  const groups: { title: string; rows: ShortcutDefinition[] }[] = [];
  for (const definition of SHORTCUT_DEFINITIONS) {
    const last = groups[groups.length - 1];
    if (last && last.title === definition.group) {
      last.rows.push(definition);
    } else {
      groups.push({ title: definition.group, rows: [definition] });
    }
  }

  return (
    <SettingsPage page="shortcuts">
      <p className="pb-2 text-xs text-chrome-muted">
        Click a key, then press the new combination. Backspace clears it, and
        Esc cancels. A key two commands want is refused with a warning.
      </p>
      {groups.map((group) => (
        <Section key={group.title} title={group.title}>
          {group.rows.map((definition) => (
            <ShortcutEditorRow key={definition.id} definition={definition} onChange={bump} />
          ))}
        </Section>
      ))}
      <Section
        title="Fixed keys"
        action={
          shortcutsAreCustomized() ? (
            <SettingsButton
              className="ml-auto px-2 py-1"
              onClick={() => {
                resetAllShortcutBindings();
                bump();
              }}
            >
              Restore defaults
            </SettingsButton>
          ) : null
        }
      >
        {FIXED_SHORTCUT_ROWS.map((row) => (
          <SettingRow key={row.label} label={row.label}>
            <Tooltip content={row.hint ?? "Part of the control itself."} side="left">
              <span className="inline-flex min-w-20 justify-center rounded-chrome bg-chrome-menu-hover px-2 py-1 text-chrome-micro text-chrome-menu-text">
                {row.keys}
              </span>
            </Tooltip>
          </SettingRow>
        ))}
      </Section>
      <p className="pb-3 pt-1 text-chrome-micro text-chrome-muted">
        Shortcuts work while the main window has the focus. They stay out of
        the way when you are typing, and keys the system already owns are not
        offered here.
      </p>
    </SettingsPage>
  );
}

/**
 * One command's row: the current key on a button that records the next press,
 * a Reset that only shows once the reader has moved the key, and the refusal
 * message when a press cannot be taken.
 */
function ShortcutEditorRow({
  definition,
  onChange,
}: {
  definition: ShortcutDefinition;
  onChange: () => void;
}) {
  const [recording, setRecording] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const binding = getEffectiveBinding(definition.id);
  const customized = !bindingIsDefault(definition.id);

  function begin() {
    setMessage(null);
    setRecording(true);
    // The button is already on screen; keeping the keyboard in it is what
    // lets the very next press be the new key.
    buttonRef.current?.focus();
  }

  function finish() {
    setRecording(false);
    buttonRef.current?.focus();
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    if (!recording) {
      // Enter and Space arrive as the button's click; the rest of the keys
      // belong to the shell until the row is recording.
      if (event.key === "Enter" || event.key === " ") {
        begin();
        event.preventDefault();
      }
      return;
    }
    // While recording, nothing the button hears reaches the window or the
    // dialog behind it — not even the Esc that would close it.
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      finish();
      setMessage(null);
      return;
    }
    if (event.key === "Backspace") {
      const problem = setShortcutBinding(definition.id, null);
      if (problem.error) {
        setMessage(problem.error);
        return;
      }
      finish();
      setMessage(null);
      onChange();
      return;
    }
    const combo = comboFromEvent(event);
    if (!combo) {
      return; // A lone Ctrl or Alt: keep waiting for the key that follows it.
    }
    const problem = setShortcutBinding(definition.id, combo);
    if (problem.error) {
      setMessage(problem.error);
      return;
    }
    finish();
    setMessage(null);
    onChange();
  }

  return (
    <div className="flex flex-col">
      <SettingRow
        label={definition.label}
        settingId={`shortcut-${definition.id}`}
      >
        <div className="flex items-center gap-1.5">
          {customized ? (
            <Tooltip content="Back to the shipped key" side="left">
              <button
                type="button"
                aria-label={`Reset the shortcut for ${definition.label}`}
                onClick={() => {
                  resetShortcutBinding(definition.id);
                  setMessage(null);
                  finish();
                  onChange();
                }}
                className="grid size-5 shrink-0 place-items-center rounded-chrome text-xs text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
              >
                ↺
              </button>
            </Tooltip>
          ) : null}
          <button
            ref={buttonRef}
            type="button"
            aria-label={
              recording
                ? `Recording the shortcut for ${definition.label}`
                : `Change the shortcut for ${definition.label}`
            }
            onClick={begin}
            onKeyDown={onKeyDown}
            onBlur={() => {
              // Walking away stops the recording; it must not yank the
              // keyboard back to the button the pointer just left.
              if (recording) {
                setRecording(false);
              }
            }}
            className={
              "min-w-20 rounded-chrome px-2 py-1 text-chrome-micro transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
              (recording
                ? "bg-chrome-menu-active text-chrome-title-text"
                : binding
                  ? "bg-chrome-menu-hover text-chrome-menu-text hover:bg-chrome-menu-active"
                  : "text-chrome-muted hover:bg-chrome-menu-hover")
            }
          >
            {recording ? "Press keys…" : binding ? formatBinding(binding) : "Not set"}
          </button>
        </div>
      </SettingRow>
      {message ? (
        <p
          role="alert"
          className="pb-1 text-right text-chrome-micro text-chrome-error"
        >
          {message}
        </p>
      ) : null}
    </div>
  );
}
