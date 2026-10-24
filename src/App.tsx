import { useEffect, useMemo, useRef, useState } from "react";

import { SettingsDialog } from "@/components/settings/settings-dialog";
import type { ShellCommand } from "@/components/shell/commands";
import { TopBar } from "@/components/shell/top-bar";
import { toolbarState } from "@/components/shell/toolbar-state";
import { useShellShortcuts } from "@/components/shell/use-shell-shortcuts";
import { AboutDialog } from "@/components/workspace/about-dialog";
import { AddDownloadDialog } from "@/components/workspace/add-download-dialog";
import { Workspace } from "@/components/workspace/workspace";
import { applyFont } from "@/lib/fonts";
import {
  onTrayCommand,
  quitApp,
  setBrowserFolder,
  setMaxConnections,
  setMinimizeToTray,
  setProxy,
  setShowCompleteDialog,
  setSpeedLimit,
} from "@/lib/ipc";
import {
  loadSettings,
  loadToolbarSize,
  saveSettings,
  saveToolbarSize,
  type ToolbarSize,
} from "@/lib/settings";
import { useDownloads } from "@/state/download-store";

export default function App() {
  const downloads = useDownloads();
  // The bar follows the list: a button whose command cannot act is dimmed
  // instead of quietly doing nothing.
  const toolbarDisabled = useMemo(
    () => toolbarState(downloads.downloads, downloads.selected),
    [downloads.downloads, downloads.selected],
  );
  const [categoriesVisible, setCategoriesVisible] = useState(true);
  const [addDownloadOpen, setAddDownloadOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [toolbarSize, setToolbarSize] = useState<ToolbarSize>(loadToolbarSize);

  // The accelerators the menus print work anywhere in the shell, but not while
  // a menu or a window of its own has the keyboard.
  useShellShortcuts({
    onAddDownload: () => setAddDownloadOpen(true),
    onFind: downloads.openFind,
    onFindNext: downloads.findNext,
  });

  // The settings the engine owns are pushed to it when the application
  // starts, so a proxy or a speed limit that was set yesterday is still in
  // force today.
  useEffect(() => {
    const settings = loadSettings();
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
    void setShowCompleteDialog(settings.showCompleteDialog).catch(() => {
      // As above.
    });
    void setBrowserFolder(settings.defaultFolder).catch(() => {
      // As above; the browser hand-over falls back to the Downloads folder.
    });
  }, []);

  // A link that the browser extension handed over opens the add dialog with
  // its address already in it.
  useEffect(() => {
    if (downloads.browserRequest) {
      setAddDownloadOpen(true);
    }
  }, [downloads.browserRequest]);

  // The tray menu drives the same commands as the menu bar. The listener is
  // subscribed once; the ref carries the current store, so a state update does
  // not tear it down and subscribe again.
  const latest = useRef(downloads);
  latest.current = downloads;
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let closed = false;
    void onTrayCommand((command) => {
      if (command === "start-queue") {
        latest.current.startQueue();
      } else if (command === "stop-queue") {
        latest.current.stopQueue();
      }
    }).then((unlisten) => {
      if (closed) {
        unlisten();
      } else {
        dispose = unlisten;
      }
    });
    return () => {
      closed = true;
      dispose?.();
    };
  }, []);

  function setToolbar(size: ToolbarSize) {
    setToolbarSize(size);
    saveToolbarSize(size);
  }

  // The face lands at once; the root variable is what every component reads.
  function chooseFont(font: string) {
    saveSettings({ ...loadSettings(), uiFont: font });
    applyFont(font);
  }

  function handleCommand(command: ShellCommand) {
    if (command.startsWith("font:")) {
      chooseFont(command.slice("font:".length));
      return;
    }

    switch (command) {
      case "exit":
        void quitApp();
        break;
      case "categories":
        setCategoriesVisible((visible) => !visible);
        break;
      case "add-download":
        setAddDownloadOpen(true);
        break;
      case "options":
        setSettingsOpen(true);
        break;
      case "about":
        setAboutOpen(true);
        break;
      case "resume":
        downloads.applyToSelection("resume");
        break;
      case "stop":
        downloads.applyToSelection("stop");
        break;
      case "delete":
        downloads.applyToSelection("delete");
        break;
      case "redownload":
        downloads.redownloadSelected();
        break;
      case "pause-all":
        downloads.pauseAll();
        break;
      case "stop-all":
        downloads.stopAll();
        break;
      case "delete-completed":
        downloads.deleteCompleted();
        break;
      case "find":
        downloads.openFind();
        break;
      case "find-next":
        downloads.findNext(1);
        break;
      case "start-queue":
        downloads.startQueue();
        break;
      case "stop-queue":
        downloads.stopQueue();
        break;
      case "arrange-order":
        downloads.sortBy("order");
        break;
      case "arrange-name":
        downloads.sortBy("name");
        break;
      case "arrange-size":
        downloads.sortBy("size");
        break;
      case "arrange-status":
        downloads.sortBy("status");
        break;
      case "arrange-eta":
        downloads.sortBy("eta");
        break;
      case "arrange-speed":
        downloads.sortBy("speed");
        break;
      case "arrange-last-try":
        downloads.sortBy("lastTry");
        break;
      case "arrange-path":
        downloads.sortBy("path");
        break;
      case "toolbar-large":
        setToolbar("large");
        break;
      case "toolbar-small":
        setToolbar("small");
        break;
      case "font-reset":
        chooseFont("googlesans");
        break;
    }
  }

  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-chrome-canvas text-chrome-toolbar-text">
      <TopBar
        onCommand={handleCommand}
        menuLabels={{
          categories: categoriesVisible ? "Hide categories" : "Show categories",
        }}
        menuChecks={{
          "toolbar-large": toolbarSize === "large",
          "toolbar-small": toolbarSize === "small",
        }}
        toolbarSize={toolbarSize}
        toolbarDisabled={toolbarDisabled}
      />
      <Workspace
        categoriesVisible={categoriesVisible}
        onAddDownload={() => setAddDownloadOpen(true)}
      />
      <AddDownloadDialog
        open={addDownloadOpen}
        onClose={() => setAddDownloadOpen(false)}
      />
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
      <AboutDialog open={aboutOpen} onClose={() => setAboutOpen(false)} />
    </div>
  );
}
