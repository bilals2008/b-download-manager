import { useCallback, useEffect, useState } from "react";

import { SettingsButton } from "@/components/settings/settings-button";
import { Section, SettingRow, SettingsPage } from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { describeError, isDesktop } from "@/lib/errors";
import {
  remotePairingCode,
  remoteRevoke,
  remoteSetEnabled,
  remoteStatus,
  type RemoteStatus,
} from "@/lib/ipc-remote";

/**
 * The phone and LAN remote.
 *
 * Off until the reader turns it on. A phone on the same home network opens
 * the address this page shows, pairs once with the one-time code, and may
 * then list, add, pause, resume and remove downloads — nothing else. The
 * connection is plain HTTP on a trusted network, and the page the phone
 * opens says so.
 */
export function RemoteSettings() {
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(() => {
    if (!isDesktop()) {
      return;
    }
    remoteStatus()
      .then((next) => {
        setStatus(next);
        setProblem(null);
      })
      .catch((failure: unknown) => {
        setProblem(describeError(failure));
      });
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  function toggle(on: boolean) {
    setBusy(true);
    setProblem(null);
    void remoteSetEnabled(on)
      .then((next) => {
        setStatus(next);
        if (!on) {
          setCode(null);
        }
      })
      .catch((failure: unknown) => {
        setProblem(describeError(failure));
      })
      .finally(() => {
        setBusy(false);
      });
  }

  function showCode() {
    setProblem(null);
    void remotePairingCode()
      .then((answer) => {
        setCode(answer?.code ?? null);
      })
      .catch((failure: unknown) => {
        setProblem(describeError(failure));
      });
  }

  function revoke(deviceId: string) {
    setProblem(null);
    void remoteRevoke(deviceId)
      .then(() => {
        reload();
      })
      .catch((failure: unknown) => {
        setProblem(describeError(failure));
      });
  }

  const enabled = status?.enabled ?? false;

  return (
    <SettingsPage page="remote">
      <Section title="Phone and LAN">
        <SettingRow settingId="setting-remote" label="Let a phone on this network drive the app">
          <Switch
            label="Let a phone on this network drive the app"
            checked={enabled}
            disabled={busy}
            onChange={toggle}
          />
        </SettingRow>
        <p className="pb-2 text-chrome-micro leading-4 text-chrome-muted">
          Off by default. A phone opens the address shown here, pairs once,
          and may list, add, pause, resume and remove downloads. The page
          speaks plain HTTP: only turn it on on a network you trust.
        </p>
        {enabled ? (
          <>
            <SettingRow settingId="setting-remote-address" label="Address">
              <span className="font-mono text-chrome-micro text-chrome-menu-text">
                {status?.address ?? ""}
              </span>
            </SettingRow>
            <SettingRow settingId="setting-remote-pair" label="Pair a phone">
              <div className="flex items-center gap-2">
                <SettingsButton onClick={showCode}>Show pairing code</SettingsButton>
                {code ? (
                  <span className="font-mono text-sm tracking-widest text-chrome-menu-text">
                    {code}
                  </span>
                ) : null}
              </div>
            </SettingRow>
            <div className="pt-1 pb-2 text-chrome-micro leading-4 text-chrome-muted">
              The code is good for one phone. On the phone, open the address,
              tap anything to pair, and type the code in.
            </div>
            {status?.devices.length ? (
              <div className="flex flex-col gap-1 pt-2">
                {status.devices.map((device) => (
                  <div
                    key={device.id}
                    className="flex flex-wrap items-center justify-between gap-2 py-1"
                  >
                    <span className="min-w-0 truncate text-xs text-chrome-title-text">
                      {device.name}
                    </span>
                    <SettingsButton onClick={() => revoke(device.id)}>
                      Forget this phone
                    </SettingsButton>
                  </div>
                ))}
              </div>
            ) : null}
          </>
        ) : null}
        {problem ? (
          <p role="alert" className="text-xs text-chrome-error">
            {problem}
          </p>
        ) : null}
      </Section>
    </SettingsPage>
  );
}
