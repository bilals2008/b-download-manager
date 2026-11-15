import { Section, SettingRow, SettingsPage } from "@/components/settings/settings-page";

/**
 * BitTorrent.
 *
 * Not out yet. The page says so instead of holding a switch that talks to
 * nothing; the switch returns when the feature does.
 */
export function TorrentSettings() {
  return (
    <SettingsPage page="torrent">
      <Section title="BitTorrent">
        <SettingRow
          settingId="setting-torrent-seed"
          label="Keep uploading after the download finishes"
        >
          <span className="text-chrome-micro text-chrome-muted">
            Coming soon
          </span>
        </SettingRow>
        <p className="pt-1 text-chrome-micro leading-4 text-chrome-muted">
          Not available yet. A magnet link or a .torrent file will be a job of
          its own, with the files, the peers and what the job is doing shown in
          its own window. Afterwards you can keep uploading to the swarm, and
          uploading costs the upload budget and the electricity.
        </p>
      </Section>
    </SettingsPage>
  );
}
