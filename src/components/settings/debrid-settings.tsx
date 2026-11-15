import { useEffect, useState } from "react";

import { SettingsButton } from "@/components/settings/settings-button";
import { TextInput } from "@/components/settings/fields";
import { Section, SettingRow, SettingsPage } from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { describeError, isDesktop } from "@/lib/errors";
import {
  debridAuto,
  debridServices,
  debridSetAuto,
  debridSignIn,
  debridSignOut,
  debridTest,
  type DebridService,
} from "@/lib/ipc-debrid";

/**
 * The debrid accounts, and the host links they unlock.
 *
 * A key lives in the credential store of the operating system and nowhere
 * else: not in the settings, not in the downloads index and not in the log.
 * This page shows the name of the account and how much paid time is left,
 * and nothing else.
 *
 * The feature is experimental. A service that is down is said plainly, and
 * the link is then fetched as it is when the reader asked for it.
 */
export function DebridSettings() {
  const [services, setServices] = useState<DebridService[]>([]);
  const [auto, setAuto] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [keys, setKeys] = useState<Record<string, string>>({});

  function reload() {
    if (!isDesktop()) {
      return;
    }
    void debridServices()
      .then(setServices)
      .catch((failure: unknown) => {
        setProblem(describeError(failure));
      });
  }

  useEffect(() => {
    reload();
    void debridAuto()
      .then(setAuto)
      .catch(() => {
        // The switch stays where it is when the engine cannot say.
      });
  }, []);

  function answer(failure: unknown) {
    setProblem(describeError(failure));
    setNotice(null);
  }

  async function signIn(service: DebridService) {
    const key = (keys[service.id] ?? "").trim();
    if (!key) {
      setProblem("Paste the key of the account first.");
      return;
    }
    setBusy(service.id);
    setProblem(null);
    try {
      const state = await debridSignIn(service.id, key);
      setNotice(
        state.premium
          ? `Signed in as ${state.user}.`
          : `Signed in as ${state.user}. The account has no paid time left.`,
      );
      setKeys((previous) => ({ ...previous, [service.id]: "" }));
      reload();
    } catch (failure) {
      answer(failure);
    } finally {
      setBusy(null);
    }
  }

  async function signOut(service: DebridService) {
    setBusy(service.id);
    setProblem(null);
    try {
      await debridSignOut(service.id);
      setNotice(`Signed out of ${service.name}.`);
      reload();
    } catch (failure) {
      answer(failure);
    } finally {
      setBusy(null);
    }
  }

  async function test(service: DebridService) {
    setBusy(service.id);
    setProblem(null);
    try {
      await debridTest(service.id);
      setNotice(`${service.name} answered.`);
    } catch (failure) {
      answer(failure);
    } finally {
      setBusy(null);
    }
  }

  function toggleAuto(on: boolean) {
    setAuto(on);
    void debridSetAuto(on).catch(answer);
  }

  return (
    <SettingsPage page="debrid">
      <Section title="Accounts">
        <p className="pb-2 text-chrome-micro leading-4 text-chrome-muted">
          Experimental. An account unlocks the host links it takes at full
          speed. A key lives in the credential store of this computer and is
          never written anywhere else.
        </p>
        {services.length === 0 && problem ? (
          <p className="text-xs text-chrome-error">{problem}</p>
        ) : null}
        {services.map((service) => (
          <div key={service.id} className="flex flex-col gap-1 py-2">
            <SettingRow settingId="setting-debrid-accounts" label={service.name}>
              <span className="text-chrome-micro text-chrome-muted">
                {service.signedIn
                  ? service.premium
                    ? service.premiumDays != null
                      ? `${service.premiumDays} days of paid time`
                      : "Paid account"
                    : "No paid time left"
                  : "Not signed in"}
              </span>
            </SettingRow>
            {service.signedIn ? (
              <div className="flex flex-wrap items-center gap-2 pb-1">
                <SettingsButton
                  disabled={busy === service.id}
                  onClick={() => void test(service)}
                >
                  Check account
                </SettingsButton>
                <SettingsButton
                  disabled={busy === service.id}
                  onClick={() => void signOut(service)}
                >
                  Sign out
                </SettingsButton>
              </div>
            ) : (
              <div className="flex flex-wrap items-center gap-2 pb-1">
                <TextInput
                  id={`debrid-key-${service.id}`}
                  mono
                  placeholder="Paste the key of the account"
                  value={keys[service.id] ?? ""}
                  onChange={(value) =>
                    setKeys((previous) => ({ ...previous, [service.id]: value }))
                  }
                />
                <SettingsButton
                  disabled={busy === service.id}
                  onClick={() => void signIn(service)}
                >
                  Sign in
                </SettingsButton>
                <a
                  href={service.tokenPage}
                  target="_blank"
                  rel="noreferrer"
                  className="text-chrome-micro text-chrome-muted underline"
                >
                  Where the key is
                </a>
              </div>
            )}
          </div>
        ))}
        {problem && services.length > 0 ? (
          <p className="text-xs text-chrome-error">{problem}</p>
        ) : null}
        {notice ? <p className="text-xs text-chrome-muted">{notice}</p> : null}
      </Section>
      <Section title="Automatic">
        <SettingRow settingId="setting-debrid-auto" label="Unlock matching host links without asking">
          <Switch
            label="Unlock matching host links without asking"
            checked={auto}
            onChange={toggleAuto}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
