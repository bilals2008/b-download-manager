import { useEffect, useState } from "react";

import { RuleList } from "@/components/settings/rule-list";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { copyText } from "@/lib/clipboard";
import { describeError, isDesktop } from "@/lib/errors";
import {
  bridgePairing,
  browserStatus,
  rotateBridgeToken,
  type BridgePairing,
  type BrowserStatus,
} from "@/lib/ipc";
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
 * The pairing code is the real control over the bridge: a request has to carry
 * it, so a web page or another local program cannot start a download by
 * guessing the port. This page shows the code the application made and which
 * extension has used it.
 */
export function BrowserSettings({ settings, onChange }: BrowserSettingsProps) {
  const [status, setStatus] = useState<BrowserStatus | null>(null);
  const [pairing, setPairing] = useState<BridgePairing | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [copied, setCopied] = useState(false);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    let active = true;
    if (!isDesktop()) {
      setProblem("This can only be checked in the desktop app.");
      return;
    }
    setChecking(true);
    // Both answers come from the same check, so the page cannot show a bridge
    // that is ready beside a pairing it has not looked at.
    Promise.all([browserStatus(), bridgePairing()])
      .then(([answer, pairingAnswer]) => {
        if (!active) return;
        setStatus(answer);
        setPairing(pairingAnswer);
        setProblem(null);
      })
      .catch((failure: unknown) => {
        if (!active) return;
        setStatus(null);
        setPairing(null);
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

  async function copyCode() {
    if (!pairing?.token) return;
    if (await copyText(pairing.token)) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } else {
      setProblem("The code could not be copied. Select it and copy it by hand.");
    }
  }

  async function makeNewCode() {
    if (!isDesktop() || rotating) return;
    setRotating(true);
    try {
      setPairing(await rotateBridgeToken());
      setStatus(null);
      setProblem(null);
      setNonce((value) => value + 1);
    } catch (failure: unknown) {
      setProblem(describeError(failure));
    } finally {
      setRotating(false);
    }
  }

  const bridge = status?.bridgeRunning ?? false;
  const seen = status?.lastContactMs ?? null;
  const paired = pairing?.pairedExtension ?? status?.pairedExtension ?? null;
  const folder = pairing?.filePath ?? "";

  return (
    <SettingsPage
      title="Browser integration"
      description="Downloads from the browser."
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
          label="Ready to receive from the browser"
          hint={`This computer only · port ${status?.port ?? 27500}`}
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
          label="Browser extension"
          hint="Only an extension with this page's code is answered."
        >
          <State
            tone={paired ? "positive" : "neutral"}
            label={
              checking && !status
                ? "Checking…"
                : paired
                  ? `Paired · ${shortOrigin(paired)}`
                  : "Not paired yet."
            }
          />
        </SettingRow>
        <SettingRow
          label="Last link received"
          hint="A link only counts once the code matches."
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

      <Section title="Pairing">
        <SettingRow
          stacked
          label="Extension code"
          htmlFor="bridge-code"
          hint="Paste it into the extension's settings page once. It is what lets the browser reach this app."
        >
          <CodeField
            id="bridge-code"
            value={pairing?.token ?? ""}
            loading={checking && !pairing}
            copied={copied}
            onCopy={() => void copyCode()}
          />
        </SettingRow>
        <SettingRow
          label="New code"
          hint={
            folder
              ? `Replaces this code at once, so the extension needs the new one. Saved in ${folder}.`
              : "Replaces this code at once, so the extension needs the new one."
          }
        >
          <SettingsButton
            onClick={() => void makeNewCode()}
            disabled={!pairing?.token || rotating || !isDesktop()}
          >
            {rotating ? "Making…" : "Make a new code"}
          </SettingsButton>
        </SettingRow>
        {!pairing?.token && !checking ? (
          <p role="alert" className="text-chrome-micro text-chrome-error">
            The app has no code, so the bridge answers nothing but a ping.
            Restart the app to make one.
          </p>
        ) : null}
      </Section>

      <Section title="From the browser">
        <SettingRow
          label="Take over downloads from the browser"
          hint="These rules win while the app is running."
        >
          <Switch
            label="Take over downloads from the browser"
            checked={settings.browserCapture}
            onChange={(checked) => onChange("browserCapture", checked)}
          />
        </SettingRow>
        <SettingRow
          label="Ask before saving"
          hint="The link waits for you to press Download."
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
          hint="Blank takes every type."
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
          hint="Covers the site and anything under it."
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
        closed.
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

/** The origin is a long identifier; the row names it without owning the line. */
function shortOrigin(origin: string): string {
  const id = origin.split("//")[1] ?? origin;
  return id.length > 18 ? `${id.slice(0, 8)}…${id.slice(-6)}` : id;
}

/**
 * The pairing code, in the shape the extension's settings page asks for:
 * selectable in one go, and copyable with one button.
 */
function CodeField({
  id,
  value,
  loading,
  copied,
  onCopy,
}: {
  id: string;
  value: string;
  loading: boolean;
  copied: boolean;
  onCopy: () => void;
}) {
  return (
    <div className="flex items-start gap-2">
      <textarea
        id={id}
        readOnly
        rows={2}
        value={loading && !value ? "…" : value}
        aria-label="Extension pairing code"
        onFocus={(event) => event.currentTarget.select()}
        className="min-w-0 flex-1 resize-none rounded-chrome border border-chrome-line bg-chrome-canvas px-2 py-1.5 font-mono text-chrome-micro leading-4 break-all text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none"
      />
      <div className="flex shrink-0 flex-col items-end gap-1">
        <SettingsButton onClick={onCopy} disabled={!value}>
          Copy
        </SettingsButton>
        <span
          role="status"
          aria-live="polite"
          className={cn(
            "text-chrome-caption text-chrome-muted",
            copied ? "opacity-100" : "opacity-0",
          )}
        >
          Copied
        </span>
      </div>
    </div>
  );
}
