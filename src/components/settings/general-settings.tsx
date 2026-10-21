import type { ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import type { Settings } from "@/lib/settings";

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
  "inline-flex h-7 shrink-0 items-center justify-center rounded-chrome border border-chrome-line px-2.5 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-45 disabled:hover:bg-transparent";

/**
 * The General page: startup, browser capture and the integration shortcuts.
 * Preferences are stored locally; the browser sections are marked Planned
 * because capture ships separately.
 */
export function GeneralSettings({ settings, onChange }: GeneralSettingsProps) {
  return (
    <div className="mx-auto flex max-w-[38rem] flex-col px-5 py-6 sm:px-6">
      <header>
        <h2 className="text-base font-medium text-chrome-title-text">General</h2>
        <p className="mt-1 text-xs text-chrome-muted">
          Choose how the download manager starts and what it captures.
        </p>
      </header>

      <div className="mt-5 flex flex-col divide-y divide-chrome-line">
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
          <ul className="flex flex-col divide-y divide-chrome-line">
            {INTEGRATIONS.map(({ label, description, action }) => (
              <li
                key={label}
                className="flex items-center justify-between gap-4 py-2.5 first:pt-0 last:pb-0"
              >
                <div className="min-w-0">
                  <p className="text-xs text-chrome-title-text">{label}</p>
                  <p className="mt-0.5 text-xs text-chrome-muted">
                    {description}
                  </p>
                </div>
                <button type="button" disabled className={OUTLINE_BUTTON}>
                  {action}
                </button>
              </li>
            ))}
          </ul>
        </Section>
      </div>
    </div>
  );
}

interface SectionProps {
  title: string;
  planned?: boolean;
  children: ReactNode;
}

function Section({ title, planned, children }: SectionProps) {
  return (
    <section aria-label={title} className="py-5 first:pt-0 last:pb-0">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-xs font-medium text-chrome-title-text">{title}</h3>
        {planned ? (
          <Badge
            variant="outline"
            className="border-chrome-line px-1.5 py-0 text-chrome-micro font-normal text-chrome-muted"
          >
            Planned
          </Badge>
        ) : null}
      </div>
      <div className="mt-3">{children}</div>
    </section>
  );
}
