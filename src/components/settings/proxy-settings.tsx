import { useEffect, useState } from "react";

import { TextInput } from "@/components/settings/fields";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { describeError, isDesktop } from "@/lib/errors";
import { validateProxy } from "@/lib/ipc";
import {
  proxyScriptState,
  reloadProxyScript,
  setProxyScript,
  type ScriptState,
} from "@/lib/ipc-proxy";
import type { Settings } from "@/lib/settings";

interface ProxySettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Proxy page. The address is checked by the engine before it is saved, so
 * a value that would fail at the first download is not kept. Proxy
 * authentication is not part of this build, and a credential in the address is
 * refused rather than stored in plain local storage.
 */
export function ProxySettings({ settings, onChange }: ProxySettingsProps) {
  const [draft, setDraft] = useState(settings.proxyUrl);
  const [synced, setSynced] = useState(settings.proxyUrl);
  const [problem, setProblem] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [script, setScript] = useState<ScriptState | null>(null);
  const [scriptDraft, setScriptDraft] = useState("");

  // What the engine has for the script, and the address it was fetched from.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    proxyScriptState()
      .then((state) => {
        if (active) {
          setScript(state);
          setScriptDraft(state.url);
        }
      })
      .catch(() => {
        // An engine that does not answer leaves the page as it is.
      });
    return () => {
      active = false;
    };
  }, []);

  // A reset from another page, or the committed value, replaces the draft.
  if (synced !== settings.proxyUrl) {
    setSynced(settings.proxyUrl);
    setDraft(settings.proxyUrl);
    setProblem(null);
  }

  // Say what the engine thinks while the address is being typed. This is
  // advice; the value is only saved once it is valid.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    setChecking(true);
    const timer = setTimeout(() => {
      validateProxy(draft)
        .then(() => {
          if (active) {
            setProblem(null);
            setChecking(false);
          }
        })
        .catch((failure: unknown) => {
          if (active) {
            setProblem(describeError(failure));
            setChecking(false);
          }
        });
    }, 350);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [draft]);

  async function commit() {
    const value = draft.trim();
    if (!isDesktop()) {
      setProblem("Proxy settings are only available in the desktop app.");
      return;
    }
    try {
      const clean = await validateProxy(value);
      setProblem(null);
      setDraft(clean);
      if (clean !== settings.proxyUrl) {
        onChange("proxyUrl", clean);
      }
    } catch (failure) {
      setProblem(describeError(failure));
    }
  }

  async function commitScript() {
    if (!isDesktop()) {
      setProblem("Proxy settings are only available in the desktop app.");
      return;
    }
    try {
      const state = await setProxyScript(scriptDraft.trim());
      setScript(state);
      setScriptDraft(state.url);
      setProblem(null);
    } catch (failure) {
      setProblem(describeError(failure));
    }
  }

  async function reloadScript() {
    try {
      const state = await reloadProxyScript();
      setScript(state);
      setProblem(null);
    } catch (failure) {
      setProblem(describeError(failure));
    }
  }

  return (
    <SettingsPage page="proxy">
      <Section title="Proxy address">
        <SettingRow
          settingId="setting-proxy"
          stacked
          label="Proxy address"
          htmlFor="proxy-address"
        >
          <TextInput
            id="proxy-address"
            mono
            value={draft}
            placeholder="socks5://127.0.0.1:1080"
            invalid={problem !== null}
            onChange={(value) => setDraft(value)}
            onBlur={() => void commit()}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void commit();
              }
            }}
          />
          <p
            aria-live="polite"
            className={
              problem
                ? "mt-1.5 break-words text-chrome-micro text-chrome-error"
                : "mt-1.5 break-words text-chrome-micro text-chrome-muted"
            }
          >
            {problem ??
              (checking
                ? "Checking the address…"
                : draft.trim()
                  ? "The address works."
                  : "No proxy. Downloads go straight to the site.")}
          </p>
          <p className="mt-1 text-chrome-micro text-chrome-muted">
            A user name or password in the address does not work.
          </p>
        </SettingRow>
      </Section>

      <Section title="Proxy auto-config">
        <SettingRow
          settingId="setting-proxy-script"
          stacked
          label="Script address"
          htmlFor="proxy-script"
        >
          <TextInput
            id="proxy-script"
            mono
            value={scriptDraft}
            placeholder="http://proxy.example.org/proxy.pac"
            invalid={problem !== null}
            onChange={(value) => setScriptDraft(value)}
            onBlur={() => void commitScript()}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void commitScript();
              }
            }}
          />
          <p className="mt-1.5 text-chrome-micro text-chrome-muted">
            {script?.note ??
              "The script decides, for every address, whether the download goes through a proxy or straight to the server."}
          </p>
          {script?.loaded ? (
            <div className="mt-2">
              <SettingsButton onClick={() => void reloadScript()}>
                Fetch again
              </SettingsButton>
            </div>
          ) : null}
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
