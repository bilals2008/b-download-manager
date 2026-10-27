import { useEffect, useRef, useState } from "react";

import { AdvancedSettings } from "@/components/settings/advanced-settings";
import { AboutSettings } from "@/components/settings/about-settings";
import { AppearanceSettings } from "@/components/settings/appearance-settings";
import { BrowserSettings } from "@/components/settings/browser-settings";
import { ConnectionsSettings } from "@/components/settings/connections-settings";
import { DownloadsSettings } from "@/components/settings/downloads-settings";
import { GeneralSettings } from "@/components/settings/general-settings";
import { ProxySettings } from "@/components/settings/proxy-settings";
import { SaveLocationsSettings } from "@/components/settings/save-locations-settings";
import {
  settingsPageLabel,
  type SettingsPageId,
} from "@/components/settings/settings-nav";
import { SettingsSidebar } from "@/components/settings/settings-sidebar";
import { Dialog } from "@/components/ui/dialog";
import { broadcastSettings } from "@/lib/app-events";
import { describeError } from "@/lib/errors";
import { applyFont, applyWeight } from "@/lib/fonts";
import {
  setBrowserConnections,
  setBrowserFolder,
  setBrowserPolicy,
  setClipboardCapture,
  setCloseToTray,
  setLaunchOnStartup,
  setMaxConnections,
  setMinimizeToTray,
  setProxy,
  setShowCompleteDialog,
  setShowProgressOnStart,
  setSpeedLimit,
} from "@/lib/ipc";
import {
  clearLastFolder,
  DEFAULT_SETTINGS,
  loadSettings,
  saveSettings,
  type Settings,
} from "@/lib/settings";
import { applyTheme } from "@/lib/theme";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

/** How long typing settles before the engine hears about it. */
const ENGINE_PUSH_DELAY = 350;

/** The settings that live in the engine, not only in this window. */
const ENGINE_KEYS: readonly (keyof Settings)[] = [
  "proxyUrl",
  "speedLimitEnabled",
  "speedLimitBytes",
  "maxConnections",
  "minimizeToTray",
  "closeToTray",
  "showCompleteDialog",
  "showProgressOnStart",
  "clipboardCapture",
  "defaultFolder",
  "defaultConnections",
  "browserCapture",
  "captureAsk",
  "captureTypes",
  "skipSites",
];

/**
 * The Settings window. Every change is written as it is made: there is no Save
 * to press and nothing to cancel, which is what makes the appearance previews
 * honest and the engine limits immediate.
 *
 * The engine owns the proxy, the speed cap, the connection ceiling and the
 * browser hand-over rules, so those are pushed to it once typing settles. The
 * startup entry is the one setting the operating system can refuse; its switch
 * follows what the system reports rather than what was asked.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [page, setPage] = useState<SettingsPageId>("general");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const pushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Two changes can land in the same tick (a switch and the value it turns
  // on). The ref carries the newest object so the second one builds on the
  // first instead of overwriting it with the render it started from.
  const latest = useRef(settings);
  latest.current = settings;

  useEffect(() => {
    if (!open) {
      return;
    }
    const saved = loadSettings();
    latest.current = saved;
    setSettings(saved);
    setError(null);
    setNotice(null);
  }, [open]);

  useEffect(
    () => () => {
      if (pushTimer.current) {
        clearTimeout(pushTimer.current);
      }
    },
    [],
  );

  function scheduleEngine(next: Settings) {
    if (pushTimer.current) {
      clearTimeout(pushTimer.current);
    }
    pushTimer.current = setTimeout(() => {
      pushTimer.current = null;
      void applyEngine(next)
        .then(() => setError(null))
        .catch((failure: unknown) => setError(describeError(failure)));
    }, ENGINE_PUSH_DELAY);
  }

  function update<K extends keyof Settings>(key: K, value: Settings[K]) {
    let next = { ...latest.current, [key]: value };
    // Turning the cap on with no number yet is a cap of zero, which means no
    // cap to the engine. Start at a usable one instead.
    if (
      key === "speedLimitEnabled" &&
      next.speedLimitEnabled &&
      next.speedLimitBytes === 0
    ) {
      next = { ...next, speedLimitBytes: 1024 * 1024 };
    }

    latest.current = next;
    setSettings(next);
    saveSettings(next);
    setNotice(null);
    // Every window reads the saved preferences: the progress window follows
    // the appearance and the speed limit, the completion window the
    // appearance. The main window applies its own change below without
    // waiting for the round trip.
    broadcastSettings();

    if (key === "theme") {
      applyTheme(next.theme);
    } else if (key === "uiFont") {
      applyFont(next.uiFont);
    } else if (key === "uiWeight") {
      applyWeight(next.uiWeight);
    }

    if (key === "launchOnStartup") {
      void applyStartup(next.launchOnStartup);
      return;
    }
    if (!ENGINE_KEYS.includes(key)) {
      return;
    }
    // A close choice has to be on disk before the next click on the close
    // button, which can arrive well inside the typing delay. It is one small
    // write, so it does not wait for the rest of the settings.
    if (key === "minimizeToTray" || key === "closeToTray") {
      const push =
        key === "minimizeToTray"
          ? setMinimizeToTray(Boolean(value))
          : setCloseToTray(Boolean(value));
      void push.catch((failure: unknown) =>
        setError(describeError(failure)),
      );
    }
    scheduleEngine(next);
  }

  async function applyStartup(requested: boolean) {
    try {
      const actual = await setLaunchOnStartup(requested);
      if (actual !== requested) {
        correctStartup(
          actual,
          requested
            ? "The system did not keep the startup entry on."
            : "The system kept the startup entry on.",
        );
        return;
      }
      setError(null);
    } catch (failure) {
      correctStartup(
        !requested,
        `The startup entry could not be changed: ${describeError(failure)}`,
      );
    }
  }

  /** Put the switch where the system really is, and say why. */
  function correctStartup(value: boolean, message: string) {
    const corrected = { ...loadSettings(), launchOnStartup: value };
    latest.current = corrected;
    setSettings(corrected);
    saveSettings(corrected);
    setError(message);
  }

  function resetAll() {
    const next: Settings = {
      ...DEFAULT_SETTINGS,
      captureTypes: [],
      skipSites: [],
    };
    latest.current = next;
    setSettings(next);
    saveSettings(next);
    clearLastFolder();
    applyTheme(next.theme);
    applyFont(next.uiFont);
    applyWeight(next.uiWeight);
    broadcastSettings();
    setError(null);
    setNotice(null);
    setPage("general");

    if (pushTimer.current) {
      clearTimeout(pushTimer.current);
      pushTimer.current = null;
    }
    void Promise.allSettled([
      applyEngine(next),
      setLaunchOnStartup(false),
    ]).then((results) => {
      const failed = results.find((result) => result.status === "rejected");
      if (failed?.status === "rejected") {
        setError(describeError(failed.reason));
      } else {
        setNotice("Settings restored to defaults.");
      }
    });
  }

  function body() {
    switch (page) {
      case "general":
        return <GeneralSettings settings={settings} onChange={update} />;
      case "appearance":
        return <AppearanceSettings settings={settings} onChange={update} />;
      case "downloads":
        return <DownloadsSettings settings={settings} onChange={update} />;
      case "save-locations":
        return <SaveLocationsSettings settings={settings} onChange={update} />;
      case "browser":
        return <BrowserSettings settings={settings} onChange={update} />;
      case "connections":
        return <ConnectionsSettings settings={settings} onChange={update} />;
      case "proxy":
        return <ProxySettings settings={settings} onChange={update} />;
      case "advanced":
        return <AdvancedSettings onReset={resetAll} />;
      case "about":
        return <AboutSettings settings={settings} onChange={update} />;
    }
  }

  return (
    <Dialog
      open={open}
      title="Settings"
      onClose={onClose}
      className="h-[min(44rem,calc(100dvh-2rem))] w-[min(56rem,calc(100vw-2rem))] max-w-none bg-chrome-panel"
      headerClassName="border-b border-chrome-line pr-1.5"
      tooltips={false}
      bodyClassName="flex min-h-0 flex-1 flex-col gap-0 p-0"
      footerClassName="items-center border-t border-chrome-line px-4 py-2.5"
      footer={
        <>
          {error ? (
            <span role="alert" className="mr-auto text-xs text-chrome-error">
              {error}
            </span>
          ) : (
            <span
              role="status"
              className="mr-auto text-chrome-micro text-chrome-muted"
            >
              {notice ?? "Changes apply as you make them."}
            </span>
          )}
          <button
            type="button"
            onClick={onClose}
            className="rounded-chrome border border-chrome-line bg-chrome-canvas px-3 py-1.5 text-xs text-chrome-menu-text transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Close
          </button>
        </>
      }
    >
      <div className="flex min-h-0 w-full flex-1">
        <SettingsSidebar page={page} onSelect={setPage} />
        <div
          role="region"
          aria-label={`${settingsPageLabel(page)} settings`}
          className="min-h-0 min-w-0 flex-1 overflow-y-auto"
        >
          {body()}
        </div>
      </div>
    </Dialog>
  );
}

/**
 * Push every engine-owned preference. They run together so a refused proxy
 * does not keep the connection limit or the tray switch from being applied.
 */
async function applyEngine(next: Settings): Promise<void> {
  const results = await Promise.allSettled([
    setProxy(next.proxyUrl),
    setSpeedLimit(next.speedLimitEnabled ? next.speedLimitBytes : 0),
    setMaxConnections(next.maxConnections),
    setMinimizeToTray(next.minimizeToTray),
    setCloseToTray(next.closeToTray),
    setShowCompleteDialog(next.showCompleteDialog),
    setShowProgressOnStart(next.showProgressOnStart),
    setClipboardCapture(next.clipboardCapture),
    setBrowserFolder(next.defaultFolder),
    setBrowserConnections(next.defaultConnections),
    setBrowserPolicy({
      capture: next.browserCapture,
      ask: next.captureAsk,
      types: next.captureTypes,
      skipSites: next.skipSites,
    }),
  ]);
  const failed = results.find((result) => result.status === "rejected");
  if (failed?.status === "rejected") {
    throw failed.reason;
  }
}
