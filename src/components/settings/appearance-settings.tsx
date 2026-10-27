import { useId } from "react";

import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { ListBox } from "@/components/ui/listbox";
import { FONT_OPTIONS, WEIGHT_OPTIONS } from "@/lib/fonts";
import type { Settings, ThemePreference, UiFont } from "@/lib/settings";

interface AppearanceSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Appearance page. Every change lands in this window as it is made, and
 * the other windows follow the saved value when they are told it changed.
 */
export function AppearanceSettings({
  settings,
  onChange,
}: AppearanceSettingsProps) {
  return (
    <SettingsPage
      title="Appearance"
      description="Theme, font and weight."
    >
      <Section title="Theme">
        <SettingRow label="Theme" hint="Auto follows Windows.">
          <Segmented
            label="Theme"
            value={settings.theme}
            onChange={(theme) => onChange("theme", theme)}
            options={THEMES}
          />
        </SettingRow>
      </Section>

      <Section title="Typography">
        <FontChoice
          value={settings.uiFont}
          onChange={(font) => onChange("uiFont", font)}
        />
        <SettingRow label="Weight" hint={WEIGHT_HINT}>
          <Segmented
            label="Weight"
            value={settings.uiWeight}
            onChange={(weight) => onChange("uiWeight", weight)}
            options={WEIGHT_OPTIONS}
          />
        </SettingRow>
        {/* Real UI text, so the face and its figures can be compared here. */}
        <p className="flex items-baseline gap-2 truncate py-2 text-xs">
          <span className="shrink-0 text-chrome-micro text-chrome-muted">
            Preview
          </span>
          <span
            aria-hidden="true"
            className="truncate tabular-nums text-chrome-menu-text"
          >
            Downloading archive.zip · 12.4 MB of 48.0 MB at 2.3 MB/s
          </span>
        </p>
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

const WEIGHT_HINT = "Every window at once.";

/**
 * A row of exclusive choices. A fieldset of radios, so arrows move between
 * them and the group is announced as one choice.
 */
function Segmented<T extends string>({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: T;
  onChange: (value: T) => void;
  options: { value: T; label: string }[];
}) {
  const name = useId();

  return (
    <fieldset
      aria-label={label}
      className="inline-flex min-w-0 rounded-chrome bg-chrome-canvas p-0.5"
    >
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
          <span className="flex h-6 items-center rounded-chrome px-2.5 text-xs text-chrome-muted transition-colors hover:text-chrome-title-text peer-checked:bg-chrome-menu-active peer-checked:text-chrome-title-text peer-focus-visible:ring-2 peer-focus-visible:ring-chrome-focus peer-focus-visible:ring-inset">
            {option.label}
          </span>
        </label>
      ))}
    </fieldset>
  );
}

/**
 * The face the window is drawn in: the faces the application ships, and the
 * note under the picker says what the chosen one is like.
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
    <SettingRow label="Font" hint={current.note}>
      <div className="w-44">
        <ListBox
          label="Font"
          value={current.value}
          onChange={(next) => onChange(next)}
          options={FONT_OPTIONS.map((option) => ({
            value: option.value,
            label: option.label,
          }))}
        />
      </div>
    </SettingRow>
  );
}
