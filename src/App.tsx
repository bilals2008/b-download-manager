import { useEffect, useMemo, useRef, useState } from "react";

import { SettingsDialog } from "@/components/settings/settings-dialog";
import type { ShellCommand } from "@/components/shell/commands";
import { TopBar } from "@/components/shell/top-bar";
import { toolbarState } from "@/components/shell/toolbar-state";
import { useShellShortcuts } from "@/components/shell/use-shell-shortcuts";
import { AboutDialog } from "@/components/workspace/about-dialog";
import { AddDownloadDialog } from "@/components/workspace/add-download-dialog";
import { CloseConfirmDialog } from "@/components/workspace/close-confirm-dialog";
import { UpdateDialog } from "@/components/workspace/update-dialog";
import { Workspace } from "@/components/workspace/workspace";
import { applyFont } from "@/lib/fonts";
import { openGrabberWindow } from "@/lib/grabber";
import {
  hideMainWindow,
  onClipboardLink,
  onCloseRequested,
  quitApp,
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
  loadSettings,
  loadToolbarSize,
  saveSettings,
  saveToolbarSize,
  type ToolbarSize,
} from "@/lib/settings";
import { runCheck, type UpdateFound } from "@/lib/updates";
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
  const [clipboardLink, setClipboardLink] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [updateOpen, setUpdateOpen] = useState(false);
  // The answer a startup check already got, so the window that shows it does
  // not ask the release server the same question a second time.
  const [startupUpdate, setStartupUpdate] = useState<UpdateFound | null>(null);
  // The count of downloads the backend would interrupt if this window exited.
  // Null when the close button has not asked anything.
  const [closeRequest, setCloseRequest] = useState<number | null>(null);
  const [toolbarSize, setToolbarSize] = useState<ToolbarSize>(loadToolbarSize);
  // The clipboard listener is subscribed once; the ref carries the current
  // dialog state so a link that arrives mid-edit cannot overwrite the field.
  const addOpenRef = useRef(addDownloadOpen);
  addOpenRef.current = addDownloadOpen;

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
    void setCloseToTray(settings.closeToTray).catch(() => {
      // As above.
    });
    void setShowCompleteDialog(settings.showCompleteDialog).catch(() => {
      // As above.
    });
    void setBrowserFolder(settings.defaultFolder).catch(() => {
      // As above; the browser hand-over falls back to the Downloads folder.
    });
    void setBrowserConnections(settings.defaultConnections).catch(() => {
      // As above; a hand-over falls back to the engine default.
    });
    void setShowProgressOnStart(settings.showProgressOnStart).catch(() => {
      // As above.
    });
    void setClipboardCapture(settings.clipboardCapture).catch(() => {
      // As above.
    });
    void setLaunchOnStartup(settings.launchOnStartup)
      .then((actual) => {
        // The system may have removed the entry or refused it. Save what it
        // reports, so the switch opens on the truth rather than on the request.
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
    // The one question the reader agreed to on the About page. Only a newer
    // release earns a window: an answer that is the current version, or one
    // that did not arrive at all, is written down and left quietly there.
    if (settings.updateAutoCheck) {
      void runCheck().then((outcome) => {
        saveSettings({ ...loadSettings(), lastUpdateCheck: Date.now() });
        if (outcome.kind === "found") {
          setStartupUpdate(outcome.update);
          setUpdateOpen(true);
        }
      });
    }
  }, []);

  // A link that the browser extension handed over opens the add dialog with
  // its address already in it.
  useEffect(() => {
    if (downloads.browserRequest) {
      setAddDownloadOpen(true);
    }
  }, [downloads.browserRequest]);

  // The close button asks instead of leaving when downloads are active. The
  // backend has already prevented the native close; this window answers the
  // question, and nothing is stopped until one of the buttons is pressed.
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let closed = false;
    void onCloseRequested((active) => setCloseRequest(active)).then((unlisten) => {
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

  function keepRunning() {
    setCloseRequest(null);
    void hideMainWindow().catch(() => {
      // The window stays where it is; the close button can ask again.
    });
  }

  function exitAnyway() {
    setCloseRequest(null);
    // The backend pauses the running downloads and writes their resumes
    // before the process ends, so this is not a loss of progress.
    void quitApp();
  }

  // A copied link opens the add dialog with its address in it. Nothing starts
  // until the reader confirms, and a link that arrives while the dialog is
  // being filled in does not overwrite the field.
  useEffect(() => {
    let dispose: (() => void) | undefined;
    let closed = false;
    void onClipboardLink((url) => {
      if (addOpenRef.current) {
        return;
      }
      setClipboardLink(url);
      setAddDownloadOpen(true);
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

  // A copied link seeds one add dialog; closing it must not seed the next one.
  function closeAddDownload() {
    setAddDownloadOpen(false);
    setClipboardLink(null);
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
      case "run-grabber":
        // The window opens beside this one, so a crawl does not block the
        // download list. A refusal is only possible if the window toolkit
        // cannot build a window, which the log records.
        void openGrabberWindow().catch(() => {});
        break;
      case "about":
        setAboutOpen(true);
        break;
      case "check-updates":
        // A request from the menu is a fresh question, whatever a startup
        // check may have found on the way in.
        setStartupUpdate(null);
        setUpdateOpen(true);
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
        seedUrl={clipboardLink}
        onClose={closeAddDownload}
      />
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
      <AboutDialog open={aboutOpen} onClose={() => setAboutOpen(false)} />
      <UpdateDialog
        open={updateOpen}
        found={startupUpdate ?? undefined}
        onClose={() => {
          setUpdateOpen(false);
          setStartupUpdate(null);
        }}
      />
      {closeRequest !== null ? (
        <CloseConfirmDialog
          active={closeRequest}
          onKeepRunning={keepRunning}
          onExit={exitAnyway}
          onCancel={() => setCloseRequest(null)}
        />
      ) : null}
    </div>
  );
}
