import { Section, SettingRow, SettingsPage } from "@/components/settings/settings-page";

/**
 * The phone and LAN remote.
 *
 * Not out yet. The page says so instead of holding a switch that talks to a
 * server nobody reaches; the switch returns when the feature does.
 */
export function RemoteSettings() {
  return (
    <SettingsPage page="remote">
      <Section title="Phone and LAN">
        <SettingRow
          settingId="setting-remote"
          label="Let a phone on this network drive the app"
        >
          <span className="text-chrome-micro text-chrome-muted">
            Coming soon
          </span>
        </SettingRow>
        <p className="pt-1 pb-2 text-chrome-micro leading-4 text-chrome-muted">
          Not available yet. A phone on this network will open an address,
          pair once with a one-time code, and may then list, add, pause,
          resume and remove downloads. The connection is plain HTTP on a
          network you trust.
        </p>
        <SettingRow settingId="setting-remote-pair" label="Pair a phone">
          <span className="text-chrome-micro text-chrome-muted">
            Coming soon
          </span>
        </SettingRow>
        <p className="pt-1 text-chrome-micro leading-4 text-chrome-muted">
          The code will be good for one phone. On the phone, open the address,
          tap anything to pair, and type the code in.
        </p>
      </Section>
    </SettingsPage>
  );
}
