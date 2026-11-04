import { useState } from "react";

import { AuthorAvatar } from "@/components/shell/author-avatar";
import { AppMark } from "@/components/shell/app-mark";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { ReleaseNotes } from "@/components/ui/release-notes";
import { Switch } from "@/components/ui/switch";
import { WarningMark } from "@/components/icons/desktop-icons";
import { UpdateProgressBody } from "@/components/updates/update-meter";
import { useAppVersion } from "@/lib/app-version";
import { APP_NAME, APP_TAGLINE } from "@/lib/brand";
import { isDesktop } from "@/lib/errors";
import type { Settings } from "@/lib/settings";
import {
  formatCheckTime,
  runCheck,
  type CheckOutcome,
  type UpdateFound,
} from "@/lib/updates";
import {
  beginUpdate,
  clearUpdateRun,
  restartApplication,
  retryUpdate,
  useUpdateRun,
  type UpdateRun,
} from "@/state/update-store";

interface AboutSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  /** Open the latest release notes, after or without the startup popup. */
  onWhatsNew: () => void;
}

/** Where the check stands between the question and its answer. */
type Check = { kind: "idle" } | { kind: "checking" } | CheckOutcome;

/**
 * About: this build, and how it keeps itself up to date.
 *
 * The version comes from the running package, so this page and the installer
 * cannot disagree. A check is made when the reader asks for one, or at startup
 * if they turned that on themselves.
 */
export function AboutSettings({
  settings,
  onChange,
  onWhatsNew,
}: AboutSettingsProps) {
  const version = useAppVersion();
  const [check, setCheck] = useState<Check>({ kind: "idle" });
  const run = useUpdateRun();
  const applying = run.phase !== "idle";

  function askAgain() {
    setCheck({ kind: "checking" });
    // The check answers with a failure rather than rejecting, so the only thing
    // left here is to write down the moment it finished.
    void runCheck().then((outcome) => {
      setCheck(outcome);
      onChange("lastUpdateCheck", Date.now());
    });
  }

  function startInstall(update: UpdateFound) {
    beginUpdate(update);
  }

  const versionLabel = version
    ? version
    : isDesktop()
      ? "Reading…"
      : "Not available in the browser preview";

  return (
    <SettingsPage page="about">
      <Section
        title="Application"
        action={
          <SettingsButton onClick={onWhatsNew}>What's new</SettingsButton>
        }
      >
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
        <SettingRow settingId="setting-version"          label="Version"
        >
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
            disabled={check.kind === "checking" || applying}
          >
            {check.kind === "checking" ? "Checking…" : "Check now"}
          </SettingsButton>
        }
      >
        <SettingRow settingId="setting-last-check"          label="Last check"
        >
          <span className="text-chrome-micro text-chrome-muted">
            {formatCheckTime(settings.lastUpdateCheck)}
          </span>
        </SettingRow>
        <SettingRow
          settingId="setting-auto-update"
          label="Check for updates when the application starts"
        >
          <Switch
            label="Check for updates when the application starts"
            checked={settings.updateAutoCheck}
            onChange={(checked) => onChange("updateAutoCheck", checked)}
          />
        </SettingRow>

        {/* One region that stays on the page, so the answer is read out at the
            moment it lands instead of only when the reader looks for it. */}
        <div aria-live={applying ? undefined : "polite"}>
          {applying ? (
            <UpdateRunAnswer run={run} />
          ) : (
            <UpdateAnswer
              check={check}
              onInstall={startInstall}
              onDismiss={() => setCheck({ kind: "idle" })}
            />
          )}
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
  onInstall: (update: UpdateFound) => void;
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
        <div className="max-h-28 overflow-y-auto scrollbar-thin rounded-chrome bg-chrome-menu-hover p-2">
          <ReleaseNotes notes={update.notes} />
        </div>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <SettingsButton tone="normal" onClick={() => onInstall(update)}>
          Update now
        </SettingsButton>
        <SettingsButton onClick={onDismiss}>Not now</SettingsButton>
      </div>
    </div>
  );
}

/** The shared surface while a download, check or install is actually running. */
function UpdateRunAnswer({ run }: { run: UpdateRun }) {
  return (
    <div className="flex flex-col gap-2 pt-2">
      {run.update ? (
        <p className="text-xs text-chrome-title-text">
          Version {run.update.version}, from {run.update.currentVersion}.
        </p>
      ) : null}
      <UpdateProgressBody run={run} />
      {run.phase === "restart-ready" ? (
        <div className="flex flex-wrap gap-2">
          <SettingsButton tone="normal" onClick={restartApplication}>
            Restart now
          </SettingsButton>
          <SettingsButton onClick={clearUpdateRun}>Later</SettingsButton>
        </div>
      ) : run.phase === "error" ? (
        <div className="flex flex-wrap gap-2">
          <SettingsButton tone="normal" onClick={retryUpdate}>
            Try again
          </SettingsButton>
          <SettingsButton onClick={clearUpdateRun}>Later</SettingsButton>
        </div>
      ) : null}
    </div>
  );
}
