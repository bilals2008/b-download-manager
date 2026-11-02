import { useId } from "react";

import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { ThemeChoice } from "@/components/settings/theme-choice";
import { ListBox } from "@/components/ui/listbox";
import { FONT_OPTIONS, WEIGHT_OPTIONS } from "@/lib/fonts";
import type { Settings, UiFont } from "@/lib/settings";

interface AppearanceSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

export function AppearanceSettings({
  settings,
  onChange,
}: AppearanceSettingsProps) {
  return (
    <SettingsPage title="Appearance">
      <Section title="Theme">
        <SettingRow
          settingId="setting-theme"
          label="Theme"
          stacked
        >
          <ThemeChoice
            value={settings.theme}
            onChange={(theme) => onChange("theme", theme)}
          />
        </SettingRow>
      </Section>

      <Section title="Typography">
        <SettingRow label="Font">
          <FontChoice
            value={settings.uiFont}
            onChange={(font) => onChange("uiFont", font)}
          />
        </SettingRow>
        <SettingRow label="Weight">
          <Segmented
            label="Weight"
            value={settings.uiWeight}
            onChange={(weight) => onChange("uiWeight", weight)}
            options={WEIGHT_OPTIONS}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}

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
    <ListBox
      label="Font"
      value={current.value}
      onChange={(next) => onChange(next)}
      options={FONT_OPTIONS.map((option) => ({
        value: option.value,
        label: option.label,
      }))}
    />
  );
}
