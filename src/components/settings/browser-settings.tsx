import { useEffect, useState } from "react";

import { RuleList } from "@/components/settings/rule-list";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { describeError, isDesktop } from "@/lib/errors";
import { browserStatus, type BrowserStatus } from "@/lib/ipc";
import {
  cleanCaptureType,
  cleanSite,
  MAX_RULE_ENTRIES,
  type Settings,
} from "@/lib/settings";
import { cn } from "@/lib/utils";

interface BrowserSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Browser integration page. The rules here are the ones the extension
 * follows while the application runs; a status line only says what the
 * application has actually observed, never that a browser is "connected".
 *
 * There is nothing to pair: the extension finds the application on its own as
 * soon as both are installed. The code this page used to show is gone, because
 * copying a 64-character secret is not a step a normal reader should meet.
 */
export function BrowserSettings({ settings, onChange }: BrowserSettingsProps) {
  const [status, setStatus] = useState<BrowserStatus | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    let active = true;
    if (!isDesktop()) {
      setProblem("This can only be checked in the desktop app.");
      return;
    }
    setChecking(true);
    browserStatus()
      .then((answer) => {
        if (!active) return;
        setStatus(answer);
        setProblem(null);
      })
      .catch((failure: unknown) => {
        if (!active) return;
        setStatus(null);
        setProblem(describeError(failure));
      })
      .finally(() => {
        if (active) {
          setChecking(false);
        }
      });
    return () => {
      active = false;
    };
  }, [nonce]);

  const bridge = status?.bridgeRunning ?? false;
  const seen = status?.lastContactMs ?? null;

  return (
    <SettingsPage page="browser">
      <Section
        title="Status"
        action={
          <SettingsButton onClick={() => setNonce((value) => value + 1)}>
            Check again
          </SettingsButton>
        }
      >
        <SettingRow
          settingId="setting-browser-ready"
          label={`Ready to receive from the browser · port ${status?.port ?? 27500}`}
        >
          <State
            tone={bridge ? "positive" : "negative"}
            label={
              checking && !status
                ? "Checking…"
                : bridge
                  ? "Ready"
                  : "Not ready"
            }
          />
        </SettingRow>
        <SettingRow
          settingId="setting-browser-last-link"
          label="Last link received"
        >
          <span className="text-chrome-micro text-chrome-menu-text">
            {checking && !status ? "Checking…" : lastLinkLabel(seen)}
          </span>
        </SettingRow>
        {problem ? (
          <p role="alert" className="text-chrome-micro text-chrome-error">
            {problem}
          </p>
        ) : null}
      </Section>

      <Section title="From the browser">
        <SettingRow
          settingId="setting-browser-capture"
          label="Take over downloads from the browser"
        >
          <Switch
            label="Take over downloads from the browser"
            checked={settings.browserCapture}
            onChange={(checked) => onChange("browserCapture", checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-browser-ask"
          label="Ask before saving"
        >
          <Switch
            label="Ask before saving"
            checked={settings.captureAsk}
            disabled={!settings.browserCapture}
            onChange={(checked) => onChange("captureAsk", checked)}
          />
        </SettingRow>
      </Section>

      <Section title="File types">
        <SettingRow
          settingId="setting-capture-types"
          stacked
          label="Take over only these types"
          htmlFor="capture-types"
        >
          <RuleList
            id="capture-types"
            label="File types"
            placeholder="mp4"
            values={settings.captureTypes}
            onChange={(values) => onChange("captureTypes", values)}
            clean={cleanCaptureType}
            format={(value) => `.${value}`}
            emptyText="Every file type"
            invalidText="Enter a file extension, for example mp4."
            max={MAX_RULE_ENTRIES}
          />
        </SettingRow>
      </Section>

      <Section title="Sites">
        <SettingRow
          settingId="setting-skip-sites"
          stacked
          label="Leave these sites to the browser"
          htmlFor="capture-sites"
        >
          <RuleList
            id="capture-sites"
            label="Sites"
            placeholder="example.org"
            values={settings.skipSites}
            onChange={(values) => onChange("skipSites", values)}
            clean={cleanSite}
            format={(value) => value}
            emptyText="No site is left out"
            invalidText="Enter a site name, for example example.org."
            max={MAX_RULE_ENTRIES}
          />
        </SettingRow>
      </Section>

      <p className="text-chrome-micro leading-4 text-chrome-muted">
        The extension connects on its own: there is no code to paste. It keeps
        its own copy of these rules for when this app is closed.
      </p>
    </SettingsPage>
  );
}

function State({
  tone,
  label,
}: {
  tone: "positive" | "negative" | "neutral";
  label: string;
}) {
  return (
    <span className="inline-flex items-center gap-1.5 text-chrome-micro text-chrome-menu-text">
      <span
        aria-hidden="true"
        className={cn(
          "size-1.5 shrink-0 rounded-full",
          tone === "positive"
            ? "bg-primary"
            : tone === "negative"
              ? "bg-chrome-error"
              : "bg-chrome-muted",
        )}
      />
      {label}
    </span>
  );
}

/** What the recorded contact means, in one line, without claiming a link. */
function lastLinkLabel(lastContactMs: number | null): string {
  if (lastContactMs === null) {
    return "Nothing received since the app started.";
  }
  const seconds = Math.max(0, Math.round((Date.now() - lastContactMs) / 1000));
  if (seconds < 10) {
    return "A link just now.";
  }
  if (seconds < 120) {
    return `Last link ${seconds} seconds ago.`;
  }
  const minutes = Math.round(seconds / 60);
  if (minutes < 120) {
    return `Last link ${minutes} minutes ago.`;
  }
  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return `Last link ${hours} hours ago.`;
  }
  const days = Math.round(hours / 24);
  return `Last link ${days} days ago.`;
}
