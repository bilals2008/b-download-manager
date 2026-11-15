import { useEffect } from "react";

import {
  pushCategoryRules,
  setBrowserConnections,
  setBrowserFolder,
  setBrowserPolicy,
  setClipboardCapture,
  setCloseToTray,
  setDeleteArchiveAfterExtract,
  setExtractAfterDone,
  setLaunchOnStartup,
  setMaxConnections,
  setMinimizeToTray,
  setProxy,
  setShowCompleteDialog,
  setShowProgressOnStart,
  setShowVideoWindowOnStart,
  setSpeedLimit,
} from "@/lib/ipc";
import { loadSettings, saveSettings, syncEngineSettings } from "@/lib/settings";

/**
 * Hand the engine the settings this window holds when the app starts.
 *
 * The settings the engine owns come back from its own file, so a proxy or a
 * speed limit that was set yesterday is still in force today; the pushes below
 * hand it the webview's copy, which repairs the file after an upgrade. The
 * file wins after that.
 */
export function useEngineSync() {
  useEffect(() => {
    void (async () => {
      let settings = loadSettings();
      try {
        const synced = await syncEngineSettings();
        // No file yet (a first run or an upgrade): the webview's copy stays
        // the seed and creates the file through the pushes.
        if (synced) {
          settings = synced;
        }
      } catch {
        // The engine is not answering; the copy below is what this window
        // has always read.
      }
      void setProxy(settings.proxyUrl).catch(() => {
        // The engine is not there yet; the settings window can set it later.
      });
      void setSpeedLimit(
        settings.speedLimitEnabled ? settings.speedLimitBytes : 0,
      ).catch(() => {
        // As above.
      });
      void setMaxConnections(settings.maxConnections).catch(() => {
        // As above.
      });
      void setMinimizeToTray(settings.minimizeToTray).catch(() => {
        // As above.
      });
      void setCloseToTray(settings.closeToTray).catch(() => {
        // As above.
      });
      void setShowCompleteDialog(settings.showCompleteDialog).catch(() => {
        // As above.
      });
      void setExtractAfterDone(settings.extractAfterDone).catch(() => {
        // As above; the engine keeps its own answer for this run.
      });
      void setDeleteArchiveAfterExtract(
        settings.deleteArchiveAfterExtract,
      ).catch(() => {
        // As above.
      });
      void setBrowserFolder(settings.defaultFolder).catch(() => {
        // As above; the browser hand-over falls back to the Downloads folder.
      });
      // The folder each category saves to is pushed before anything can be
      // added — including from the browser extension. The folder for the bytes
      // of an unfinished download belongs to the application, not to the
      // reader.
      void pushCategoryRules().catch(() => {
        // As above; the engine keeps the folder it started with.
      });
      void setBrowserConnections(settings.defaultConnections).catch(() => {
        // As above; a hand-over falls back to the engine default.
      });
      void setShowProgressOnStart(settings.showProgressOnStart).catch(() => {
        // As above.
      });
      void setShowVideoWindowOnStart(settings.showVideoWindowOnStart).catch(
        () => {
          // As above.
        },
      );
      void setClipboardCapture(settings.clipboardCapture).catch(() => {
        // As above.
      });
      void setLaunchOnStartup(settings.launchOnStartup)
        .then((actual) => {
          // The system may have removed the entry or refused it. Save what it
          // reports, so the switch opens on the truth rather than on the
          // request.
          if (actual !== settings.launchOnStartup) {
            saveSettings({ ...loadSettings(), launchOnStartup: actual });
          }
        })
        .catch(() => {
          // The switch shows its own failure the next time it is changed.
        });
      void setBrowserPolicy({
        capture: settings.browserCapture,
        ask: settings.captureAsk,
        types: settings.captureTypes,
        skipSites: settings.skipSites,
      }).catch(() => {
        // The extension falls back to its own rules when the bridge is closed.
      });
    })();
  }, []);
}
