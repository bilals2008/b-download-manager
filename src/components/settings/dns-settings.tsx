import { useEffect, useState } from "react";

import { TextInput } from "@/components/settings/fields";
import { SettingsButton } from "@/components/settings/settings-button";
import { Section, SettingRow } from "@/components/settings/settings-page";
import { ListBox } from "@/components/ui/listbox";
import { Switch } from "@/components/ui/switch";
import { describeError, isDesktop } from "@/lib/errors";
import {
  dnsSettings,
  setDns,
  testDns,
  type DnsMode,
  type DnsSettings,
} from "@/lib/ipc-dns";

/** The ways names can be resolved, as the picker offers them. */
const MODES: { value: DnsMode; label: string }[] = [
  { value: "system", label: "The system's resolver" },
  { value: "doh", label: "DNS over HTTPS" },
  { value: "dot", label: "DNS over TLS" },
  { value: "nextdns", label: "NextDNS" },
];

/** What the engine has, before anything is read. */
const EMPTY: DnsSettings = {
  mode: "system",
  dohUrl: "",
  dotHost: "",
  nextdnsProfile: "",
  fallback: false,
};

/** The field the mode uses. */
function fieldOf(dns: DnsSettings): string {
  switch (dns.mode) {
    case "doh":
      return dns.dohUrl;
    case "dot":
      return dns.dotHost;
    case "nextdns":
      return dns.nextdnsProfile;
    default:
      return "";
  }
}

/**
 * The DNS the engine asks names with.
 *
 * The engine owns the choice: it is stored in the engine's settings file and
 * put in force for every client — downloads, the grabber, video tracks, the
 * update check. This page reads that file and writes to it, so what the
 * window shows and what the engine does cannot drift apart.
 *
 * The endpoint is checked by the engine before it is kept, so a value that
 * could never be asked is refused here rather than failing at the first
 * download. A mode whose endpoint is still missing stays a draft on this
 * page: the field to fill it in appears, and the engine keeps answering the
 * way it did until the value is good.
 *
 * The test button asks one name through the resolver as it is typed, and
 * says how long it took. It never falls back to the system's resolver, so a
 * dead resolver cannot look alive.
 */
export function DnsSettings() {
  const [dns, setLocal] = useState<DnsSettings>(EMPTY);
  const [mode, setMode] = useState<DnsMode>("system");
  const [draft, setDraft] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const [answer, setAnswer] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);

  // The engine's file is the owner: the page opens on what it holds.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    dnsSettings()
      .then((saved) => {
        if (active) {
          setLocal(saved);
          setMode(saved.mode);
          setDraft(fieldOf(saved));
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

  /** The values as they stand on this page: the mode picked and the field. */
  function current(): DnsSettings {
    const next: DnsSettings = { ...dns, mode };
    const value = draft.trim();
    switch (mode) {
      case "doh":
        next.dohUrl = value;
        break;
      case "dot":
        next.dotHost = value;
        break;
      case "nextdns":
        next.nextdnsProfile = value;
        break;
    }
    return next;
  }

  /** Keep the engine's values, and put the new one in force. */
  async function commit(next: DnsSettings): Promise<void> {
    if (!isDesktop()) {
      setProblem("DNS settings are only available in the desktop app.");
      return;
    }
    try {
      const clean = await setDns(next);
      setLocal(clean);
      setMode(clean.mode);
      setDraft(fieldOf(clean));
      setProblem(null);
      setAnswer(null);
    } catch (failure) {
      setProblem(describeError(failure));
    }
  }

  /** A mode is picked: the field for it appears, and the engine is told. */
  function choose(picked: DnsMode): void {
    setMode(picked);
    setDraft(picked === mode ? draft : fieldOf({ ...dns, mode: picked }));
    setAnswer(null);
    void commit({ ...dns, mode: picked });
  }

  async function run(): Promise<void> {
    if (!isDesktop()) {
      setProblem("DNS settings are only available in the desktop app.");
      return;
    }
    setTesting(true);
    setAnswer(null);
    try {
      const result = await testDns(current());
      setProblem(null);
      setAnswer(
        `Answered in ${result.milliseconds} ms: ${result.addresses.join(", ")}`,
      );
    } catch (failure) {
      setProblem(describeError(failure));
    } finally {
      setTesting(false);
    }
  }

  return (
    <Section title="Names (DNS)">
      <SettingRow
        settingId="setting-dns-mode"
        label="How names are resolved"
      >
        <div className="w-64">
          <ListBox
            label="How names are resolved"
            value={mode}
            onChange={(value) => choose(value as DnsMode)}
            options={MODES}
          />
        </div>
      </SettingRow>

      {mode === "doh" ? (
        <SettingRow
          settingId="setting-dns-endpoint"
          stacked
          label="DNS over HTTPS address"
          htmlFor="dns-endpoint"
        >
          <TextInput
            id="dns-endpoint"
            mono
            value={draft}
            placeholder="https://cloudflare-dns.com/dns-query"
            invalid={problem !== null}
            onChange={(value) => setDraft(value)}
            onBlur={() => void commit(current())}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void commit(current());
              }
            }}
          />
          <p className="mt-1.5 text-chrome-micro text-chrome-muted">
            The address must be https:// and must not carry a user name or a
            password. Its own name is looked up once through the system, so
            the first lookup is not yet fully private.
          </p>
        </SettingRow>
      ) : null}

      {mode === "dot" ? (
        <SettingRow
          settingId="setting-dns-hostname"
          stacked
          label="DNS over TLS hostname"
          htmlFor="dns-hostname"
        >
          <TextInput
            id="dns-hostname"
            mono
            value={draft}
            placeholder="one.one.one.one"
            invalid={problem !== null}
            onChange={(value) => setDraft(value)}
            onBlur={() => void commit(current())}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void commit(current());
              }
            }}
          />
          <p className="mt-1.5 text-chrome-micro text-chrome-muted">
            The name the server's certificate must show, asked at port 853. Its
            address is looked up once through the system.
          </p>
        </SettingRow>
      ) : null}

      {mode === "nextdns" ? (
        <SettingRow
          settingId="setting-dns-profile"
          stacked
          label="NextDNS configuration"
          htmlFor="dns-profile"
        >
          <TextInput
            id="dns-profile"
            mono
            value={draft}
            placeholder="Leave empty for the default"
            invalid={problem !== null}
            onChange={(value) => setDraft(value)}
            onBlur={() => void commit(current())}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void commit(current());
              }
            }}
          />
          <p className="mt-1.5 text-chrome-micro text-chrome-muted">
            The six hex digits of the configuration, as NextDNS shows them. The
            name is asked over TLS as {"{id}"}.dns.nextdns.io.
          </p>
        </SettingRow>
      ) : null}

      <SettingRow
        settingId="setting-dns-fallback"
        label="Use the system's resolver when the chosen one cannot be reached"
      >
        <Switch
          label="Use the system's resolver when the chosen one cannot be reached"
          checked={dns.fallback}
          onChange={(checked) => void commit({ ...dns, fallback: checked })}
        />
      </SettingRow>

      <SettingRow settingId="setting-dns-test" label="Try the resolver">
        <SettingsButton onClick={() => void run()} disabled={testing}>
          {testing ? "Asking…" : "Test"}
        </SettingsButton>
      </SettingRow>

      {problem ? (
        <p role="alert" className="text-chrome-micro text-chrome-error">
          {problem}
        </p>
      ) : null}
      {answer ? (
        <p role="status" className="text-chrome-micro text-chrome-muted">
          {answer}
        </p>
      ) : null}
    </Section>
  );
}
