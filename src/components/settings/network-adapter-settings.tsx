import { useEffect, useState } from "react";

import { Section, SettingRow } from "@/components/settings/settings-page";
import { ListBox } from "@/components/ui/listbox";
import { describeError, isDesktop } from "@/lib/errors";
import { savedSettings } from "@/lib/ipc";
import {
  networkAdapters,
  setNetworkAdapter,
  type NetworkAdapter,
} from "@/lib/ipc-network";

/**
 * The network adapter every connection leaves through.
 *
 * The reader picks a connection — a VPN, usually — and the engine holds every
 * socket to it. When that adapter goes away the downloads stop rather than
 * quietly taking the normal route, so this page says plainly that the
 * application watches for it.
 *
 * The choice is kept by the adapter's identity, not by its name, so renaming
 * a connection in Windows keeps the choice.
 */
export function NetworkAdapterSettings() {
  const [adapters, setAdapters] = useState<NetworkAdapter[]>([]);
  const [choice, setChoice] = useState("");
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    // The engine's file is the owner of the choice; the page opens on what it
    // holds.
    savedSettings()
      .then((saved) => {
        if (active && saved) {
          setChoice(saved.networkAdapter ?? "");
        }
      })
      .catch(() => {
        // An engine that does not answer leaves the page on the default.
      });
    networkAdapters()
      .then((list) => {
        if (active) {
          setAdapters(list);
        }
      })
      .catch((failure: unknown) => {
        if (active) {
          setProblem(describeError(failure));
        }
      });
    return () => {
      active = false;
    };
  }, []);

  async function choose(id: string): Promise<void> {
    if (!isDesktop()) {
      setProblem("Network settings are only available in the desktop app.");
      return;
    }
    try {
      await setNetworkAdapter(id);
      setChoice(id);
      setProblem(null);
    } catch (failure) {
      setProblem(describeError(failure));
    }
  }

  const options = [
    { value: "", label: "The system's route" },
    ...adapters.map((adapter) => ({
      value: adapter.id,
      label: adapter.up
        ? `${adapter.name}${adapter.address ? ` — ${adapter.address}` : ""}`
        : `${adapter.name} — not connected`,
    })),
  ];

  return (
    <Section title="Network adapter">
      <SettingRow
        settingId="setting-network-adapter"
        label="Leave through"
      >
        <div className="w-72">
          <ListBox
            label="Leave through"
            value={choice}
            onChange={(value) => void choose(value)}
            options={options}
          />
        </div>
      </SettingRow>
      <p className="pb-2 text-xs leading-5 text-chrome-muted">
        Every download leaves through this adapter. When it goes away the
        downloads stop instead of taking the normal route. Windows can still
        send a packet by other means: only a firewall rule can promise that
        nothing escapes.
      </p>
      {problem ? (
        <p role="alert" className="text-chrome-micro text-chrome-error">
          {problem}
        </p>
      ) : null}
    </Section>
  );
}
