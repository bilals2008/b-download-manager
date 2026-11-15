import { useEffect, useState } from "react";

import { Section, SettingRow, SettingsPage } from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { describeError, isDesktop } from "@/lib/errors";
import { setTorrentSeeding, torrentSeeding } from "@/lib/ipc-torrent";

/**
 * BitTorrent.
 *
 * A magnet link or a `.torrent` file is a job of its own: the files, the
 * peers and pieces are the job's, not a row of the list. Seeding is off by
 * default — uploading after the file is complete is a choice, and the
 * reader sees the cost before they make it.
 */
export function TorrentSettings() {
  const [seeding, setSeeding] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    void torrentSeeding()
      .then(setSeeding)
      .catch(() => {
        // The switch stays off when the engine cannot say.
      });
  }, []);

  function toggle(on: boolean) {
    setSeeding(on);
    setProblem(null);
    void setTorrentSeeding(on).catch((failure: unknown) => {
      setProblem(describeError(failure));
      setSeeding(!on);
    });
  }

  return (
    <SettingsPage page="torrent">
      <Section title="BitTorrent">
        <SettingRow
          settingId="setting-torrent-seed"
          label="Keep uploading after the download finishes"
        >
          <Switch
            label="Keep uploading after the download finishes"
            checked={seeding}
            onChange={toggle}
          />
        </SettingRow>
        <p className="pt-1 pb-2 text-chrome-micro leading-4 text-chrome-muted">
          Off by default. A finished download keeps uploading to the swarm
          until the reader stops it, and uploading costs the upload budget
          and the electricity. Turn it on only when that is the point.
        </p>
        <p className="text-chrome-micro leading-4 text-chrome-muted">
          Torrents open in the torrent window, with the files, the peers and
          what the job is doing. The files land in a folder named after the
          torrent, and the piece order is the plain order, so a film can be
          played while it comes in.
        </p>
        {problem ? (
          <p role="alert" className="pt-2 text-xs text-chrome-error">
            {problem}
          </p>
        ) : null}
      </Section>
    </SettingsPage>
  );
}
