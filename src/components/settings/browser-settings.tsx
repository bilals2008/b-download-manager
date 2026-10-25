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
 */
export function BrowserSettings({ settings, onChange }: BrowserSettingsProps) {
  const [status, setStatus] = useState<BrowserStatus | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    let active = true;
    if (!isDesktop()) {
      setProblem("The browser bridge can only be checked in the desktop app.");
      return;
    }
    setChecking(true);
    browserStatus()
      .then((answer) => {
        if (active) {
          setStatus(answer);
          setProblem(null);
        }
      })
      .catch((failure: unknown) => {
        if (active) {
          setStatus(null);
          setProblem(describeError(failure));
        }
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
    <SettingsPage
      title="Browser integration"
      description="How the browser extension hands downloads over, and the rules it follows."
    >
      <Section
        title="Status"
        action={
          <SettingsButton onClick={() => setNonce((value) => value + 1)}>
            Check again
          </SettingsButton>
        }
      >
        <SettingRow
          label="Application bridge"
          hint={`127.0.0.1:${status?.port ?? 27500} · API ${status?.api ?? 1}`}
        >
          <State
            tone={bridge ? "positive" : "negative"}
            label={
              checking && !status
                ? "Checking…"
                : bridge
                  ? "Listening"
                  : "Not listening"
            }
          />
        </SettingRow>
        <SettingRow
          label="Browser extension"
          hint="Only a request that carries the extension header is counted."
        >
          <State
            tone="neutral"
            label={checking && !status ? "Checking…" : contactLabel(seen)}
          />
        </SettingRow>
        {problem ? (
          <p role="alert" className="text-chrome-micro text-chrome-error">
            {problem}
          </p>
        ) : null}
      </Section>

      <Section title="Hand-over">
        <SettingRow
          label="Take over downloads from the browser"
          hint="While this app is running, these rules replace the extension's own copy."
        >
          <Switch
            label="Take over downloads from the browser"
            checked={settings.browserCapture}
            onChange={(checked) => onChange("browserCapture", checked)}
          />
        </SettingRow>
        <SettingRow
          label="Ask before saving"
          hint="The Add URL dialog opens with the link; nothing starts until you confirm."
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
          stacked
          label="Take over only these types"
          htmlFor="capture-types"
          hint="Leave the list empty to take over every file type."
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
          stacked
          label="Leave these sites to the browser"
          htmlFor="capture-sites"
          hint="A subdomain follows its domain. A link you pick from the menu still goes to the app."
        >
          <RuleList
            id="capture-sites"
            label="Sites"
            placeholder="example.org"
            values={settings.skipSites}
            onChange={(values) => onChange("skipSites", values)}
            clean={cleanSite}
            emptyText="No site is left out"
            invalidText="Enter a site name, for example example.org."
            max={MAX_RULE_ENTRIES}
          />
        </SettingRow>
      </Section>

      <p className="text-chrome-micro leading-4 text-chrome-muted">
        The extension keeps its own copy of these rules for when this app is
        closed. Its settings page is the fallback, not a second source while the
        app is running.
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
function contactLabel(lastContactMs: number | null): string {
  if (lastContactMs === null) {
    return "No request since the app started.";
  }
  const seconds = Math.max(0, Math.round((Date.now() - lastContactMs) / 1000));
  if (seconds < 10) {
    return "A request just now.";
  }
  if (seconds < 120) {
    return `Last request ${seconds} seconds ago.`;
  }
  const minutes = Math.round(seconds / 60);
  if (minutes < 120) {
    return `Last request ${minutes} minutes ago.`;
  }
  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return `Last request ${hours} hours ago.`;
  }
  const days = Math.round(hours / 24);
  return `Last request ${days} days ago.`;
}
