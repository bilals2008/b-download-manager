import { Checkbox } from "@/components/ui/checkbox";
import { HINT, LABEL } from "@/components/settings/fields";
import { Section, SettingsPage } from "@/components/settings/settings-page";
import type { Settings, ThemePreference } from "@/lib/settings";

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
      description="Choose how the download manager starts and what it captures."
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
        <AppearanceChoice
          value={settings.theme}
          onChange={(theme) => onChange("theme", theme)}
        />
      </Section>

      <Section title="Startup">
        <div className="flex flex-col gap-2.5">
          <Checkbox
            label="Launch Internet Download Manager on startup"
            hint="Open the app when you sign in."
            checked={settings.launchOnStartup}
            onChange={(event) =>
              onChange("launchOnStartup", event.target.checked)
            }
          />
          <Checkbox
            label="Start downloading URLs copied to the clipboard"
            hint="Pick up a link as soon as it is copied."
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
          hint="The tray icon brings the window back, and it is the way to exit. The icon sits at the right of the task bar."
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

/** As a segmented radio group, so arrows move between them. */
function AppearanceChoice({
  value,
  onChange,
}: {
  value: ThemePreference;
  onChange: (theme: ThemePreference) => void;
}) {
  return (
    <fieldset>
      <legend className={LABEL}>Theme</legend>
      <div className="inline-flex rounded-chrome bg-chrome-canvas p-0.5">
        {THEMES.map((option) => (
          <label key={option.value} className="cursor-pointer">
            <input
              type="radio"
              name="theme"
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
      <p className={HINT}>
        Auto follows the system appearance. The window changes as soon as you
        pick a theme, and Save keeps it.
      </p>
    </fieldset>
  );
}
