import { useId } from "react";

import { Checkbox } from "@/components/ui/checkbox";
import { ListBox } from "@/components/ui/listbox";
import { HINT, LABEL } from "@/components/settings/fields";
import { Section, SettingsPage } from "@/components/settings/settings-page";
import { FONT_OPTIONS, SIZE_OPTIONS, WEIGHT_OPTIONS } from "@/lib/fonts";
import type { Settings, ThemePreference, UiFont } from "@/lib/settings";

interface GeneralSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/** The browsers IDM watches, in the order its General page lists them. */
const BROWSERS = [
  "Apple Safari",
  "Google Chrome",
  "Internet Explorer",
  "Microsoft Edge",
  "Microsoft Edge Legacy",
  "Mozilla Firefox",
  "Opera",
];

const INTEGRATIONS = [
  {
    label: "Keyboard shortcuts",
    description: "Prevent or force a download from the keyboard.",
    action: "Set keys",
  },
  {
    label: "Browser context menu",
    description: "Add download commands to the browser menu.",
    action: "Edit",
  },
  {
    label: "Download panel",
    description: "Show the download panel inside browsers.",
    action: "Edit",
  },
];

const OUTLINE_BUTTON =
  "inline-flex h-7 shrink-0 items-center justify-center rounded-chrome bg-chrome-canvas px-2.5 text-xs text-chrome-menu-text transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-45 disabled:hover:bg-chrome-canvas";

/**
 * The General page: startup, browser capture and the integration shortcuts.
 * Preferences are stored locally; the browser sections are marked Planned
 * because capture ships separately.
 */
export function GeneralSettings({ settings, onChange }: GeneralSettingsProps) {
  return (
    <SettingsPage
      title="General"
      description="Startup, appearance and browser capture."
    >
      <Section title="Browser integration">
        <div className="flex items-center justify-between gap-4">
          <p className="flex items-center gap-2 text-xs text-chrome-title-text">
            <span
              aria-hidden="true"
              className="size-1.5 rounded-full bg-chrome-muted"
            />
            Not connected
          </p>
          <button type="button" disabled className={OUTLINE_BUTTON}>
            Restart
          </button>
        </div>
        <p className="mt-2 text-xs text-chrome-muted">
          Browser integration is not part of this build yet.
        </p>
      </Section>

      <Section title="Appearance">
        <Segmented
          legend="Theme"
          hint="Auto follows Windows."
          value={settings.theme}
          onChange={(theme) => onChange("theme", theme)}
          options={THEMES}
        />
        <FontChoice
          value={settings.uiFont}
          onChange={(font) => onChange("uiFont", font)}
        />
        <div className="mt-5 grid grid-cols-1 gap-x-6 gap-y-5 sm:grid-cols-2">
          <Segmented
            legend="Weight"
            hint={WEIGHT_HINT}
            value={settings.uiWeight}
            onChange={(weight) => onChange("uiWeight", weight)}
            options={WEIGHT_OPTIONS}
          />
          <Segmented
            legend="Text size"
            hint={SIZE_HINT}
            value={settings.uiSize}
            onChange={(size) => onChange("uiSize", size)}
            options={SIZE_OPTIONS}
          />
        </div>
        <p className="mt-5 truncate rounded-chrome bg-chrome-canvas px-2.5 py-2 text-xs tabular-nums text-chrome-menu-text">
          Downloading archive.zip · 12.4 MB of 48.0 MB at 2.3 MB/s
        </p>
      </Section>

      <Section title="Startup">
        <div className="flex flex-col gap-2.5">
          <Checkbox
            label="Launch Internet Download Manager on startup"
            checked={settings.launchOnStartup}
            onChange={(event) =>
              onChange("launchOnStartup", event.target.checked)
            }
          />
          <Checkbox
            label="Start downloading URLs copied to the clipboard"
            checked={settings.clipboardCapture}
            onChange={(event) =>
              onChange("clipboardCapture", event.target.checked)
            }
          />
        </div>
      </Section>

      <Section title="The tray">
        <Checkbox
          label="Keep the application in the tray when the window is closed"
          hint="Closing the window hides it to the tray, and the tray icon brings it back."
          checked={settings.minimizeToTray}
          onChange={(event) => onChange("minimizeToTray", event.target.checked)}
        />
      </Section>

      <Section title="Capture from browsers" planned>
        <div className="grid grid-cols-1 gap-x-6 gap-y-1.5 sm:grid-cols-2">
          {BROWSERS.map((browser) => (
            <Checkbox key={browser} label={browser} defaultChecked disabled />
          ))}
        </div>
        <button type="button" disabled className={OUTLINE_BUTTON + " mt-3"}>
          Add browser...
        </button>
      </Section>

      <Section title="Integration" planned>
        <ul className="flex flex-col gap-1">
          {INTEGRATIONS.map(({ label, description, action }) => (
            <li
              key={label}
              className="flex items-center justify-between gap-4 py-2.5 first:pt-0 last:pb-0"
            >
              <div className="min-w-0">
                <p className="text-xs text-chrome-title-text">{label}</p>
                <p className="mt-0.5 text-xs text-chrome-muted">{description}</p>
              </div>
              <button type="button" disabled className={OUTLINE_BUTTON}>
                {action}
              </button>
            </li>
          ))}
        </ul>
      </Section>
    </SettingsPage>
  );
}

/** The three appearances, in the order the group shows them. */
const THEMES: { value: ThemePreference; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

const WEIGHT_HINT = "One step heavier or lighter, everywhere.";

const SIZE_HINT = "Type only, so the layout keeps its density.";

/** As a segmented radio group, so arrows move between them. */
function Segmented<T extends string>({
  legend,
  hint,
  value,
  onChange,
  options,
}: {
  legend: string;
  hint: string;
  value: T;
  onChange: (value: T) => void;
  options: { value: T; label: string }[];
}) {
  const name = useId();

  return (
    <fieldset>
      <legend className={LABEL}>{legend}</legend>
      <div className="inline-flex rounded-chrome bg-chrome-canvas p-0.5">
        {options.map((option) => (
          <label key={option.value} className="cursor-pointer">
            <input
              type="radio"
              name={name}
              value={option.value}
              checked={value === option.value}
              onChange={() => onChange(option.value)}
              className="peer sr-only"
            />
            <span className="flex h-6 items-center rounded-chrome px-2.5 text-xs text-chrome-muted hover:text-chrome-title-text peer-checked:bg-chrome-menu-active peer-checked:text-chrome-title-text peer-focus-visible:ring-2 peer-focus-visible:ring-chrome-focus peer-focus-visible:ring-inset">
              {option.label}
            </span>
          </label>
        ))}
      </div>
      <p className={HINT}>{hint}</p>
    </fieldset>
  );
}

/**
 * The face the window is drawn in. The line under the picker is real UI text,
 * so the figures and the spacing can be compared before the window is closed.
 */
function FontChoice({
  value,
  onChange,
}: {
  value: UiFont;
  onChange: (font: UiFont) => void;
}) {
  const current =
    FONT_OPTIONS.find((option) => option.value === value) ?? FONT_OPTIONS[0];

  return (
    <div className="mt-5">
      <span className={LABEL}>Font</span>
      <ListBox
        label="Font"
        value={value}
        onChange={(next) => onChange(next as UiFont)}
        options={FONT_OPTIONS.map((option) => ({
          value: option.value,
          label: option.label,
        }))}
      />
      <p className={HINT}>{current.note}</p>
    </div>
  );
}
