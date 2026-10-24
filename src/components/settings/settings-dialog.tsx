import { useEffect, useRef, useState } from "react";

import { DownloadsSettings } from "@/components/settings/downloads-settings";
import { GeneralSettings } from "@/components/settings/general-settings";
import { NetworkSettings } from "@/components/settings/network-settings";
import { PlannedSettings } from "@/components/settings/planned-settings";
import {
  OPEN_PAGES,
  PLANNED_TAB,
  type SettingsTabId,
} from "@/components/settings/settings-nav";
import { SettingsGlyph } from "@/components/icons/ui-icons";
import { Dialog } from "@/components/ui/dialog";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { broadcastSettings } from "@/lib/app-events";
import { describeError } from "@/lib/errors";
import { applyFont, applyWeight } from "@/lib/fonts";
import { cn } from "@/lib/utils";
import {
  setBrowserFolder,
  setMaxConnections,
  setMinimizeToTray,
  setProxy,
  setShowCompleteDialog,
  setSpeedLimit,
} from "@/lib/ipc";
import { DEFAULT_SETTINGS, loadSettings, saveSettings, type Settings } from "@/lib/settings";
import { applyTheme } from "@/lib/theme";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

/** How long typing settles before the engine hears about it. */
const ENGINE_PUSH_DELAY = 300;

/** The settings the engine owns; only these are pushed over IPC. */
const ENGINE_KEYS: readonly (keyof Settings)[] = [
  "proxyUrl",
  "speedLimitEnabled",
  "speedLimitBytes",
  "maxConnections",
  "minimizeToTray",
  "showCompleteDialog",
  "defaultFolder",
];

/**
 * The Settings window. It opens on
 * the General page and writes every change as it is made: there is no Save to
 * press and nothing to cancel, which is what makes the appearance previews
 * honest.
 *
 * The settings the engine owns are pushed to it once typing settles, so a
 * proxy address is not sent on every keystroke.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [page, setPage] = useState<SettingsTabId>("general");
  const [error, setError] = useState<string | null>(null);
  const pushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (open) {
      setSettings(loadSettings());
      setError(null);
    }
  }, [open]);

  useEffect(
    () => () => {
      if (pushTimer.current) {
        clearTimeout(pushTimer.current);
      }
    },
    [],
  );

  function update<K extends keyof Settings>(key: K, value: Settings[K]) {
    const next = { ...settings, [key]: value };
    setSettings(next);
    saveSettings(next);

    // Every window reads the saved preferences: the progress window follows
    // the appearance and the application-wide speed limit, the completion
    // window the appearance. The main window is the preview for its own copy,
    // and applies the change below without waiting for the round trip.
    broadcastSettings();

    // The appearance lands at once; the window is the preview.
    if (key === "theme") {
      applyTheme(next.theme);
    } else if (key === "uiFont") {
      applyFont(next.uiFont);
    } else if (key === "uiWeight") {
      applyWeight(next.uiWeight);
    }

    if (!ENGINE_KEYS.includes(key)) {
      return;
    }
    if (pushTimer.current) {
      clearTimeout(pushTimer.current);
    }
    pushTimer.current = setTimeout(() => void pushEngine(next), ENGINE_PUSH_DELAY);
  }

  async function pushEngine(next: Settings) {
    try {
      await setProxy(next.proxyUrl);
      await setSpeedLimit(next.speedLimitEnabled ? next.speedLimitBytes : 0);
      await setMaxConnections(next.maxConnections);
      await setMinimizeToTray(next.minimizeToTray);
      await setShowCompleteDialog(next.showCompleteDialog);
      await setBrowserFolder(next.defaultFolder);
      setError(null);
    } catch (failure) {
      setError(describeError(failure));
    }
  }

  return (
    <Dialog
      open={open}
      title="Settings"
      onClose={onClose}
      className="h-[min(42rem,calc(100dvh-2rem))] w-[min(52rem,calc(100vw-2rem))] max-w-none bg-chrome-panel"
      headerClassName="border-b border-chrome-line pr-1.5"
      tooltips={false}
      bodyClassName="flex min-h-0 flex-col gap-0 p-0"
      footerClassName="items-center border-t border-chrome-line px-4 py-2.5"
      footer={
        <>
          <span className="mr-auto text-chrome-micro text-chrome-muted">
            Changes apply as you make them.
          </span>
          {error ? (
            <span role="alert" className="mr-2 text-xs text-chrome-error">
              {error}
            </span>
          ) : null}
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
      <Tabs
        value={page}
        onValueChange={setPage as (value: string) => void}
        className="min-h-0 w-full flex-1"
      >
        <TabsList className="w-full">
          {OPEN_PAGES.map(({ id, label, glyph }) => (
            <TabsTrigger key={id} value={id}>
              <SettingsGlyph
                kind={glyph}
                className={cn(
                  "size-4 shrink-0",
                  page === id ? "text-chrome-focus" : "text-chrome-muted",
                )}
              />
              <span className="ml-1.5 shrink-0 truncate">{label}</span>
            </TabsTrigger>
          ))}
          <TabsTrigger value={PLANNED_TAB}>
            <SettingsGlyph
              kind="planned"
              className={cn(
                "size-4 shrink-0",
                page === PLANNED_TAB
                  ? "text-chrome-focus"
                  : "text-chrome-muted",
              )}
            />
            <span className="ml-1.5 shrink-0 truncate">Planned</span>
          </TabsTrigger>
        </TabsList>

        <TabsContent value="general">
          <GeneralSettings settings={settings} onChange={update} />
        </TabsContent>
        <TabsContent value="downloads">
          <DownloadsSettings settings={settings} onChange={update} />
        </TabsContent>
        <TabsContent value="network">
          <NetworkSettings settings={settings} onChange={update} />
        </TabsContent>
        <TabsContent value={PLANNED_TAB}>
          <PlannedSettings />
        </TabsContent>
      </Tabs>
    </Dialog>
  );
}
