import { useEffect, useState } from "react";

import { FIELD, NumberInput } from "@/components/settings/fields";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { describeError, isDesktop } from "@/lib/errors";
import {
  setTorrentSettings,
  torrentPortMapping,
  torrentSettings,
  type PortMappingStatus,
  type TorrentSettings as TorrentSettingsValues,
} from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

/**
 * The choices before the engine has answered.
 *
 * Nothing is shared and nothing is limited: a page that could not read the
 * saved values must not claim a torrent is uploading to the swarm.
 */
const INITIAL: TorrentSettingsValues = {
  seeding: false,
  uploadLimitKbps: 0,
  seedRatio: 0,
  seedMinutes: 0,
  port: 0,
  dhtNodes: [],
  lpd: true,
  portMapping: false,
};

/** What the router's port mapping is doing, in one plain line. */
function mappingSentence(status: PortMappingStatus | null): string | null {
  if (!status) {
    return null;
  }
  switch (status.state) {
    case "searching":
      return "Looking for the router…";
    case "mapped":
      return `The router opened port ${status.externalPort} (${status.method === "upnp" ? "UPnP" : "NAT-PMP"}).`;
    case "noGateway":
      return "No router on this network answered.";
    case "refused":
      return status.message;
    default:
      return null;
  }
}

/**
 * A share ratio with two decimals.
 *
 * A ratio is a fraction — twice what came in is `2.00` — so the whole-number
 * field would round away the only values that mean anything between one and
 * two. The typed text is kept until the field is left, so it can be cleared
 * and retyped.
 */
function RatioInput({
  id,
  value,
  onChange,
  disabled,
}: {
  id: string;
  value: number;
  onChange: (value: number) => void;
  disabled?: boolean;
}) {
  const [draft, setDraft] = useState(() => value.toFixed(2));
  const [shown, setShown] = useState(value);

  if (shown !== value) {
    setShown(value);
    setDraft(value.toFixed(2));
  }

  const parsed = draft.trim() === "" ? Number.NaN : Number(draft);
  const invalid =
    draft.trim() !== "" && (!Number.isFinite(parsed) || parsed < 0 || parsed > 100);

  function commit() {
    if (!Number.isFinite(parsed)) {
      setDraft(value.toFixed(2));
      return;
    }
    const rounded = Math.round(Math.min(100, Math.max(0, parsed)) * 100) / 100;
    setDraft(rounded.toFixed(2));
    if (rounded !== value) {
      onChange(rounded);
    }
  }

  return (
    <div className="flex items-center gap-2">
      <input
        id={id}
        type="number"
        inputMode="decimal"
        step={0.01}
        min={0}
        max={100}
        disabled={disabled}
        value={draft}
        aria-invalid={invalid || undefined}
        className={cn(
          FIELD,
          "w-24 text-right tabular-nums",
          invalid && "border-chrome-danger hover:border-chrome-danger",
        )}
        onChange={(event) => {
          const raw = event.target.value;
          setDraft(raw);
          const next = Number(raw);
          if (
            raw.trim() !== "" &&
            Number.isFinite(next) &&
            next >= 0 &&
            next <= 100
          ) {
            onChange(Math.round(next * 100) / 100);
          }
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.currentTarget.blur();
          }
        }}
      />
      <span className="text-xs text-chrome-muted">ratio</span>
    </div>
  );
}

/**
 * The extra bootstrap nodes, as one line of `host:port` texts separated by
 * commas. The typed text is kept until the field is left, so it can be
 * cleared and retyped; the engine cleans the list when it is saved.
 */
function NodesInput({
  id,
  value,
  onChange,
  disabled,
}: {
  id: string;
  value: string[];
  onChange: (value: string[]) => void;
  disabled?: boolean;
}) {
  const [draft, setDraft] = useState(() => value.join(", "));
  const [shown, setShown] = useState(value);

  if (shown !== value) {
    setShown(value);
    setDraft(value.join(", "));
  }

  function commit() {
    const next = draft
      .split(",")
      .map((node) => node.trim())
      .filter((node) => node.length > 0);
    setDraft(next.join(", "));
    if (next.join(",") !== value.join(",")) {
      onChange(next);
    }
  }

  return (
    <input
      id={id}
      type="text"
      spellCheck={false}
      placeholder="router.example.org:6881"
      disabled={disabled}
      value={draft}
      className={cn(FIELD, "w-64")}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.currentTarget.blur();
        }
      }}
    />
  );
}

/**
 * BitTorrent: what a finished download does with its upload, and how fast.
 *
 * The values live in the engine's settings file, not in the window: a torrent
 * that is seeding when the window closes follows the same numbers, so this
 * page reads them from the engine and writes them back whole. A page that
 * cannot reach the engine says so and leaves the sharing off.
 */
export function TorrentSettings() {
  const [settings, setSettings] = useState(INITIAL);
  const [problem, setProblem] = useState<string | null>(null);
  const [mapping, setMapping] = useState<PortMappingStatus | null>(null);
  const desktop = isDesktop();

  useEffect(() => {
    if (!desktop) {
      return;
    }
    let live = true;
    void torrentSettings()
      .then((saved) => {
        if (live) {
          setSettings(saved);
        }
      })
      .catch((failure: unknown) => {
        if (live) {
          setProblem(describeError(failure));
        }
      });
    return () => {
      live = false;
    };
  }, [desktop]);

  // The mapping is asked for in the background; while the choice is on, the
  // line under it follows what the router is actually doing.
  useEffect(() => {
    if (!desktop || !settings.portMapping) {
      return;
    }
    let live = true;
    const read = () => {
      void torrentPortMapping()
        .then((status) => {
          if (live) {
            setMapping(status);
          }
        })
        .catch(() => {
          // A page that cannot reach the engine says nothing new.
        });
    };
    read();
    const timer = setInterval(read, 5000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [desktop, settings.portMapping]);

  /**
   * Keep a change. The switch only shows on once the engine has taken it: a
   * save that fails leaves the sharing off, because a switch that says yes
   * while nothing uploads is a lie about the reader's connection.
   */
  function save(next: TorrentSettingsValues) {
    setSettings(next);
    setProblem(null);
    void setTorrentSettings(next)
      .then((saved) => {
        setSettings(saved);
      })
      .catch((failure: unknown) => {
        setProblem(describeError(failure));
        setSettings((current) => ({ ...current, seeding: false }));
      });
  }

  return (
    <SettingsPage page="torrent">
      <Section title="Sharing">
        <p className="pb-2 text-chrome-micro leading-4 text-chrome-muted">
          A finished torrent can keep uploading to the swarm. It costs upload
          traffic, so it stops at a ratio or a number of minutes; zero never
          stops.
        </p>
        <SettingRow
          settingId="setting-torrent-seed"
          label="Keep uploading after the download finishes"
        >
          <Switch
            label="Keep uploading after the download finishes"
            checked={settings.seeding}
            disabled={!desktop}
            onChange={(checked) => save({ ...settings, seeding: checked })}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-torrent-ratio"
          htmlFor="torrent-seed-ratio"
          label="Stop sharing at a share ratio of"
        >
          <RatioInput
            id="torrent-seed-ratio"
            value={settings.seedRatio}
            disabled={!desktop}
            onChange={(ratio) => save({ ...settings, seedRatio: ratio })}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-torrent-minutes"
          htmlFor="torrent-seed-minutes"
          label="Stop sharing after sharing for"
        >
          <NumberInput
            id="torrent-seed-minutes"
            value={settings.seedMinutes}
            min={0}
            max={525600}
            suffix="minutes"
            disabled={!desktop}
            onChange={(minutes) => save({ ...settings, seedMinutes: minutes })}
          />
        </SettingRow>
      </Section>

      <Section title="Upload">
        <SettingRow
          settingId="setting-torrent-upload"
          htmlFor="torrent-upload-limit"
          label="Limit uploading to"
        >
          <NumberInput
            id="torrent-upload-limit"
            value={settings.uploadLimitKbps}
            min={0}
            max={1000000}
            suffix="KB/s"
            disabled={!desktop}
            onChange={(limit) =>
              save({ ...settings, uploadLimitKbps: limit })
            }
          />
        </SettingRow>
        {problem ? (
          <p className="pt-2 text-xs text-chrome-error">{problem}</p>
        ) : null}
      </Section>

      <Section title="Peer discovery">
        <p className="pb-2 text-chrome-micro leading-4 text-chrome-muted">
          A torrent whose trackers are unreachable finds its peers through the
          distributed table. The standard bootstrap nodes are always used;
          these are extra ones, for a network where the standard ones cannot
          be reached.
        </p>
        <SettingRow
          settingId="setting-torrent-dht-nodes"
          htmlFor="torrent-dht-nodes"
          label="Extra bootstrap nodes"
        >
          <NodesInput
            id="torrent-dht-nodes"
            value={settings.dhtNodes}
            disabled={!desktop}
            onChange={(nodes) => save({ ...settings, dhtNodes: nodes })}
          />
        </SettingRow>
      </Section>

      <Section title="Local network">
        <SettingRow
          settingId="setting-torrent-lpd"
          label="Find peers on this network"
        >
          <Switch
            label="Find peers on this network"
            checked={settings.lpd}
            disabled={!desktop}
            onChange={(checked) => save({ ...settings, lpd: checked })}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-torrent-port-mapping"
          label="Open the torrent port on the router"
        >
          <Switch
            label="Open the torrent port on the router"
            checked={settings.portMapping}
            disabled={!desktop}
            onChange={(checked) => save({ ...settings, portMapping: checked })}
          />
        </SettingRow>
        {settings.portMapping && mappingSentence(mapping) ? (
          <p className="pt-2 text-xs text-chrome-muted">{mappingSentence(mapping)}</p>
        ) : null}
        <p className="pt-2 text-chrome-micro leading-4 text-chrome-muted">
          Trackers use the proxy from Options when one is set; peer connections
          and UDP trackers go directly.
        </p>
      </Section>
    </SettingsPage>
  );
}
