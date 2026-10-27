import { useState } from "react";

import { AuthorAvatar } from "@/components/shell/author-avatar";
import { AppMark } from "@/components/shell/app-mark";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { WarningMark } from "@/components/icons/desktop-icons";
import { useAppVersion } from "@/lib/app-version";
import { APP_NAME, APP_TAGLINE } from "@/lib/brand";
import { isDesktop } from "@/lib/errors";
import type { Settings } from "@/lib/settings";
import {
  describeFailure,
  formatCheckTime,
  installUpdate,
  runCheck,
  type CheckOutcome,
} from "@/lib/updates";

interface AboutSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/** Where the page stands between the question and the installer. */
type Check =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "installing" }
  | CheckOutcome;

/**
 * About: this build, and how it keeps itself up to date.
 *
 * The version comes from the running package, so this page and the installer
 * cannot disagree. A check is made when the reader asks for one, or at startup
 * if they turned that on themselves.
 */
export function AboutSettings({ settings, onChange }: AboutSettingsProps) {
  const version = useAppVersion();
  const [check, setCheck] = useState<Check>({ kind: "idle" });

  function askAgain() {
    setCheck({ kind: "checking" });
    // The check answers with a failure rather than rejecting, so the only thing
    // left here is to write down the moment it finished.
    void runCheck().then((outcome) => {
      setCheck(outcome);
      onChange("lastUpdateCheck", Date.now());
    });
  }

  function install() {
    setCheck({ kind: "installing" });
    void installUpdate().then(
      () => {
        // On success the installer takes the process over and this never runs.
        setCheck({ kind: "idle" });
      },
      (failure: unknown) => {
        setCheck({ kind: "error", message: describeFailure(failure) });
      },
    );
  }

  const versionLabel = version
    ? version
    : isDesktop()
      ? "Reading…"
      : "Not available in the browser preview";

  return (
    <SettingsPage title="About" description="This build and how it updates.">
      <Section title="Application">
        <div className="flex items-center justify-between gap-4 py-2">
          <div className="flex min-w-0 items-center gap-3">
            <AppMark className="size-9" />
            <div className="min-w-0">
              <p className="text-sm text-chrome-title-text">{APP_NAME}</p>
              <p className="text-chrome-micro leading-4 text-chrome-muted">
                {APP_TAGLINE}
              </p>
            </div>
          </div>
          <AuthorAvatar />
        </div>
        <SettingRow label="Version">
          <span className="font-mono text-chrome-micro text-chrome-menu-text">
            {versionLabel}
          </span>
        </SettingRow>
      </Section>

      <Section
        title="Updates"
        action={
          <SettingsButton
            onClick={askAgain}
            disabled={check.kind === "checking" || check.kind === "installing"}
          >
            {check.kind === "checking" ? "Checking…" : "Check now"}
          </SettingsButton>
        }
      >
        <SettingRow label="Last check">
          <span className="text-chrome-micro text-chrome-muted">
            {formatCheckTime(settings.lastUpdateCheck)}
          </span>
        </SettingRow>
        <SettingRow label="Check for updates when the application starts">
          <Switch
            label="Check for updates when the application starts"
            checked={settings.updateAutoCheck}
            onChange={(checked) => onChange("updateAutoCheck", checked)}
          />
        </SettingRow>

        {/* One region that stays on the page, so the answer is read out at the
            moment it lands instead of only when the reader looks for it. */}
        <div aria-live="polite">
          <UpdateAnswer
            check={check}
            onInstall={install}
            onDismiss={() => setCheck({ kind: "idle" })}
          />
        </div>
      </Section>
    </SettingsPage>
  );
}

/** The answer to the one question the release server is asked. */
function UpdateAnswer({
  check,
  onInstall,
  onDismiss,
}: {
  check: Check;
  onInstall: () => void;
  onDismiss: () => void;
}) {
  if (check.kind === "idle") return null;

  if (check.kind === "checking") {
    return (
      <p className="pt-1 text-xs text-chrome-menu-text">
        Looking for a newer version…
      </p>
    );
  }

  if (check.kind === "installing") {
    return (
      <p className="pt-1 text-xs text-chrome-menu-text">
        Getting the new version and checking that it is genuine…
      </p>
    );
  }

  if (check.kind === "latest") {
    return (
      <p className="pt-1 text-xs text-chrome-menu-text">
        Version {check.version} is the newest release.
      </p>
    );
  }

  if (check.kind === "error") {
    return (
      <p className="flex items-start gap-1.5 pt-1 text-xs">
        <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
        <span className="min-w-0 text-chrome-error">{check.message}</span>
      </p>
    );
  }

  const { update } = check;

  return (
    <div className="flex flex-col gap-2 pt-2">
      <p className="text-xs text-chrome-title-text">
        Version {update.version} is available.
      </p>
      {update.notes ? (
        <p className="max-h-28 overflow-y-auto whitespace-pre-wrap rounded-chrome border border-chrome-line bg-chrome-canvas p-2 text-xs text-chrome-menu-text">
          {update.notes}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <SettingsButton tone="normal" onClick={onInstall}>
          Download and install
        </SettingsButton>
        <SettingsButton onClick={onDismiss}>Not now</SettingsButton>
      </div>
    </div>
  );
}
