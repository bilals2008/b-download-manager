import { useEffect, useState } from "react";

import { SettingsDialog } from "@/components/settings/settings-dialog";
import type { ShellCommand } from "@/components/shell/commands";
import { TopBar } from "@/components/shell/top-bar";
import { AboutDialog } from "@/components/workspace/about-dialog";
import { AddDownloadDialog } from "@/components/workspace/add-download-dialog";
import { Workspace } from "@/components/workspace/workspace";
import {
  onTrayCommand,
  quitApp,
  setMaxConnections,
  setMinimizeToTray,
  setProxy,
  setShowCompleteDialog,
  setSpeedLimit,
} from "@/lib/ipc";
import {
  loadSettings,
  loadToolbarSize,
  saveToolbarSize,
  type ToolbarSize,
} from "@/lib/settings";
import { watchTheme } from "@/lib/theme";
import { useDownloads } from "@/state/download-store";

export default function App() {
  const downloads = useDownloads();
  const [categoriesVisible, setCategoriesVisible] = useState(true);
  const [addDownloadOpen, setAddDownloadOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [toolbarSize, setToolbarSize] = useState<ToolbarSize>(loadToolbarSize);

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
  }, []);

  // A link that the browser extension handed over opens the add dialog with
  // its address already in it.
  useEffect(() => {
    if (downloads.browserRequest) {
      setAddDownloadOpen(true);
    }
  }, [downloads.browserRequest]);

  // Auto keeps up with the operating system while the window is open.
  useEffect(() => watchTheme(), []);

  // The tray menu drives the same commands as the menu bar.
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let closed = false;
    void onTrayCommand((command) => {
      if (command === "start-queue") {
        downloads.startQueue();
      } else if (command === "stop-queue") {
        downloads.stopQueue();
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
  }, [downloads]);

  function setToolbar(size: ToolbarSize) {
    setToolbarSize(size);
    saveToolbarSize(size);
  }

  function handleCommand(command: ShellCommand) {
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
