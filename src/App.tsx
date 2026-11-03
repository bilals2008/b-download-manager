import { useEffect, useMemo, useRef, useState } from "react";

import { SettingsDialog } from "@/components/settings/settings-dialog";
import type { ShellCommand } from "@/components/shell/commands";
import { CustomizeToolbarDialog } from "@/components/shell/customize-toolbar-dialog";
import { ToolbarsDialog } from "@/components/shell/toolbars-dialog";
import { TopBar } from "@/components/shell/top-bar";
import { toolbarState } from "@/components/shell/toolbar-state";
import { UrlListDialog } from "@/components/shell/url-list-dialog";
import { useShellShortcuts } from "@/components/shell/use-shell-shortcuts";
import { cycleSectionFocus } from "@/components/shell/section-focus";
import { TipOfTheDayDialog } from "@/components/tips/tip-of-the-day-dialog";
import { AboutDialog } from "@/components/workspace/about-dialog";
import { AddDownloadDialog } from "@/components/workspace/add-download-dialog";
import { CloseConfirmDialog } from "@/components/workspace/close-confirm-dialog";
import { UpdateDialog } from "@/components/workspace/update-dialog";
import { Workspace } from "@/components/workspace/workspace";
import { WhatsNewDialog } from "@/components/whats-new/whats-new-dialog";
import { useWhatsNew } from "@/components/whats-new/use-whats-new";
import { APP_HOME_PAGE, APP_NAME, APP_TAGLINE } from "@/lib/brand";
import { describeError, isDesktop } from "@/lib/errors";
import { applyFont } from "@/lib/fonts";
import { openGrabberWindow } from "@/lib/grabber";
import {
  defaultDestination,
  desktopPreferences,
  hideMainWindow,
  onClipboardLink,
  onCloseRequested,
  openPath,
  openUrl,
  quitApp,
  pushDownloadFolders,
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
  setTrayDarkMode,
  setTrayHidden,
  setUrlProtocols,
  shareToSystem,
} from "@/lib/ipc";
import {
  loadSettings,
  loadToolbarSize,
  saveSettings,
  saveToolbarSize,
  syncEngineSettings,
  type ToolbarSize,
} from "@/lib/settings";
import { shareApp } from "@/lib/share";
import { nextTipIndex, saveTipCursor } from "@/lib/tips";
import {
  DEFAULT_TRAY_PREFERENCES,
  normalizeUrlProtocols,
  type TrayPreferences,
  trayPreferencesFrom,
} from "@/lib/tray";
import {
  BUILTIN_TOOLBAR_PACKS,
  DEFAULT_TOOLBAR_PACK,
  defaultToolbarLayoutFor,
  type ToolbarPack,
} from "@/lib/toolbar-packs";
import { listToolbarPacks, type ToolbarPacksSnapshot } from "@/lib/toolbar-packs-api";
import {
  DEFAULT_TOOLBAR_ID,
  loadToolbarPreference,
  resolveToolbarLayout,
  saveToolbarPreference,
  withActiveToolbar,
  withToolbarLabels,
  withToolbarLayout,
  type ToolbarPreference,
  type ToolbarToken,
} from "@/lib/toolbars";
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
  const [tipsOpen, setTipsOpen] = useState(false);
  const [tipIndex, setTipIndex] = useState(0);
  // A tip is offered once per start, and never while another dialog has the
  // reader's attention.
  const startupTipAsked = useRef(false);
  // The answer a startup check already got, so the window that shows it does
  // not ask the release server the same question a second time.
  const [startupUpdate, setStartupUpdate] = useState<UpdateFound | null>(null);
  // The count of downloads the backend would interrupt if this window exited.
  // Null when the close button has not asked anything.
  const [closeRequest, setCloseRequest] = useState<number | null>(null);
  const [toolbarSize, setToolbarSize] = useState<ToolbarSize>(loadToolbarSize);
  // The bar is a pack plus a layout. The pack paints it and may suggest a
  // button order; the layout is what this reader chose on this machine.
  const [toolbarPreference, setToolbarPreference] =
    useState<ToolbarPreference>(loadToolbarPreference);
  const [toolbarPacks, setToolbarPacks] = useState<ToolbarPacksSnapshot>({
    folder: "",
    packs: [],
    problems: [],
  });
  const [customizeToolbarOpen, setCustomizeToolbarOpen] = useState(false);
  const [toolbarsOpen, setToolbarsOpen] = useState(false);
  // The tray choices belong to the backend, which owns the icon and reads them
  // before this window has drawn anything. The menu shows what it saved.
  const [tray, setTray] = useState<TrayPreferences>(DEFAULT_TRAY_PREFERENCES);
  const [urlListOpen, setUrlListOpen] = useState(false);

  // A version's notes are offered once, after the list has loaded and every
  // transfer has stopped. A busy window never loses focus to a popup.
  const activeDownloads = useMemo(
    () => downloads.downloads.filter((item) => item.status === "downloading").length,
    [downloads.downloads],
  );  const whatsNew = useWhatsNew({
    loading: downloads.loading,
    activeDownloads,
    blocked:
      addDownloadOpen ||
      settingsOpen ||
      aboutOpen ||
      tipsOpen ||
      updateOpen ||
      closeRequest !== null ||
      customizeToolbarOpen ||
      toolbarsOpen ||
      urlListOpen ||
      downloads.conflict !== null ||
      downloads.pendingDelete !== null ||
      Boolean(downloads.browserRequest),
  });

  const allToolbars = useMemo<ToolbarPack[]>(
    () => [...BUILTIN_TOOLBAR_PACKS, ...toolbarPacks.packs],
    [toolbarPacks.packs],
  );
  // A remembered pack can disappear between runs; the default bar is always
  // there to fall back to.
  const activeToolbar =
    allToolbars.find((pack) => pack.id === toolbarPreference.active) ??
    DEFAULT_TOOLBAR_PACK;
  const toolbarLayout = resolveToolbarLayout(
    toolbarPreference,
    activeToolbar.id,
    activeToolbar.buttons,
  );
  const toolbarMenu = useMemo(
    () => allToolbars.map((pack) => ({ id: pack.id, label: pack.name })),
    [allToolbars],
  );
  const showToolbarLabels = toolbarPreference.labels;
  const toolbarChecks = useMemo<Partial<Record<ShellCommand, boolean>>>(
    () => ({
      "toolbar-large": toolbarSize === "large",
      "toolbar-small": toolbarSize === "small",
      [`toolbar-pack:${activeToolbar.id}`]: true,
      "tray-hidden": tray.hidden,
      "tray-dark-mode": tray.darkMode,
    }),
    [toolbarSize, activeToolbar.id, tray],
  );
  // The clipboard listener is subscribed once; the ref carries the current
  // dialog state so a link that arrives mid-edit cannot overwrite the field.
  const addOpenRef = useRef(addDownloadOpen);
  addOpenRef.current = addDownloadOpen;

  // The keys come from the shortcut registry, so a rebind in Settings takes
  // effect on the next press. They work anywhere in the shell, but not while
  // a menu or a window of its own has the keyboard.
  useShellShortcuts({
    "add-download": () => setAddDownloadOpen(true),
    find: downloads.openFind,
    "find-next": () => downloads.findNext(1),
    "find-previous": () => downloads.findNext(-1),
    "open-downloads-folder": openDownloadsFolder,
    "pause-all": downloads.pauseAll,
    "resume-all": downloads.resumeAll,
    "stop-all": downloads.stopAll,
    "pause-selected": () => downloads.applyToSelection("pause"),
    "resume-selected": () => downloads.applyToSelection("resume"),
    "cancel-selected": () => downloads.applyToSelection("stop"),
    "remove-selected": () => downloads.applyToSelection("delete"),
    "select-all": downloads.selectAll,
    "clear-selection": downloads.clearSelection,
    "start-queue": downloads.startQueue,
    "stop-queue": downloads.stopQueue,
    "open-settings": () => setSettingsOpen(true),
    "cycle-sections": () => cycleSectionFocus(1),
    "cycle-sections-back": () => cycleSectionFocus(-1),
  });

  /** Show the folder new downloads land in, the way Explorer would open it. */
  function openDownloadsFolder() {
    if (!isDesktop()) {
      return;
    }
    void defaultDestination()
      .then((folder) => openPath(folder))
      .catch((failure: unknown) => downloads.reportError(describeError(failure)));
  }

  // The settings the engine owns come back from its own file when the app
  // starts, so a proxy or a speed limit that was set yesterday is still in
  // force today; the pushes below hand it the webview's copy, which repairs
  // the file after an upgrade. The file wins after that.
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
    void setBrowserFolder(settings.defaultFolder).catch(() => {
      // As above; the browser hand-over falls back to the Downloads folder.
    });
    // The parts of a download are placed the moment it starts, so the
    // unfinished folder and the folder each category saves to are pushed
    // before anything can be added — including from the browser extension.
    void pushDownloadFolders(settings.unfinishedFolder).catch(() => {
      // As above; the engine keeps the folder it started with.
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
    })();
  }, []);

  // Toolbar packs live in a folder beside the application's data. They are
  // read once at start; opening the View menu's "Look for new toolbars" reads
  // the folder again, so a file dropped there by hand shows up at once.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let cancelled = false;
    void listToolbarPacks()
      .then((snapshot) => {
        if (!cancelled) {
          setToolbarPacks(snapshot);
        }
      })
      .catch((failure: unknown) => {
        if (!cancelled) {
          setToolbarPacks((current) => ({
            ...current,
            problems: [{ file: "toolbars", message: describeError(failure) }],
          }));
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // The tray choices are kept with the close button's, in the backend's own
  // file: the icon has to be right from the first second, before this page has
  // read anything. The menu opens on what it actually holds.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    void desktopPreferences()
      .then((saved) => setTray(trayPreferencesFrom(saved)))
      .catch(() => {
        // The defaults are what the icon was built with, so showing them while
        // the backend is not answering is not a lie.
      });
  }, []);

  // The one question the reader agreed to on the About page. It runs once the
  // window is up: only a newer release earns a dialog; an answer that is the
  // current version, or one that did not arrive at all, is written down and
  // left quietly there. A launch that is introducing this version holds the
  // check back, so two offers cannot open one after the other.
  useEffect(() => {
    if (!isDesktop() || !whatsNew.ready || whatsNew.freshNotes) {
      return;
    }
    if (!loadSettings().updateAutoCheck) {
      return;
    }
    void runCheck().then((outcome) => {
      saveSettings({ ...loadSettings(), lastUpdateCheck: Date.now() });
      if (outcome.kind === "found") {
        setStartupUpdate(outcome.update);
        setUpdateOpen(true);
      }
    });
  }, [whatsNew.ready, whatsNew.freshNotes]);

  // A tip is offered once per start, and never over the add dialog the browser
  // just asked for, or over the notes that explain an update.
  useEffect(() => {
    if (
      !isDesktop() ||
      !whatsNew.ready ||
      whatsNew.freshNotes ||
      startupTipAsked.current
    ) {
      return;
    }
    startupTipAsked.current = true;
    if (!loadSettings().tipsOnStartup || addOpenRef.current) {
      return;
    }
    openTips();
  }, [whatsNew.ready, whatsNew.freshNotes]);

  // A link that the browser extension handed over opens the add dialog with
  // its address already in it.
  useEffect(() => {
    if (downloads.browserRequest) {
      setAddDownloadOpen(true);
      // The reader is already looking at a download; the tip can wait.
      setTipsOpen(false);
    }
  }, [downloads.browserRequest]);

  // The close button asks instead of leaving when downloads are active. The
  // backend has already prevented the native close; this window answers the
  // question, and nothing is stopped until one of the buttons is pressed.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
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
    if (!isDesktop()) {
      return;
    }
    let dispose: (() => void) | undefined;
    let closed = false;
    void onClipboardLink((url) => {
      if (addOpenRef.current) {
        return;
      }
      setClipboardLink(url);
      setAddDownloadOpen(true);
      setTipsOpen(false);
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

  // Switching the pack repaints the bar. The reader's button order for that
  // pack is kept, so going back and forth does not undo customization.
  function switchToolbarPack(id: string) {
    const next = withActiveToolbar(toolbarPreference, id);
    setToolbarPreference(next);
    saveToolbarPreference(next);
  }

  function saveToolbarLayout(layout: ToolbarToken[]) {
    const next = withToolbarLayout(toolbarPreference, activeToolbar.id, layout);
    setToolbarPreference(next);
    saveToolbarPreference(next);
  }

  function setToolbarLabels(show: boolean) {
    const next = withToolbarLabels(toolbarPreference, show);
    setToolbarPreference(next);
    saveToolbarPreference(next);
  }

  /**
   * Move one tray choice. The backend owns the icon, so the request goes
   * there; the menu shows the choice the row was given.
   */
  function changeTray(next: TrayPreferences, request: () => Promise<unknown>) {
    setTray(next);
    if (!isDesktop()) {
      return;
    }
    void request().catch(() => {
      // The same fall-back every other backend setting uses: the choice stands
      // for this run and the next open reads what is on disk.
    });
  }

  // With no icon there is no way back from a hidden window, so closing the
  // window quits instead. `lifecycle::close_action` already decides that from
  // whether the icon is there.
  function chooseTrayHidden(hidden: boolean) {
    changeTray({ ...tray, hidden }, () => setTrayHidden(hidden));
  }

  function chooseTrayDarkMode(darkMode: boolean) {
    changeTray({ ...tray, darkMode }, () => setTrayDarkMode(darkMode));
  }

  /** The protocols the clipboard watch offers a copied address for. */
  function saveUrlProtocols(protocols: string[]) {
    changeTray(
      { ...tray, urlProtocols: normalizeUrlProtocols(protocols) },
      () => setUrlProtocols(protocols),
    );
  }

  // Both the View menu and the Toolbar page of the options window open the
  // same two dialogs, and both pick up a pack dropped into the folder since
  // the window opened.
  function openToolbarManager() {
    setToolbarsOpen(true);
    void refreshToolbarPacks().catch((failure: unknown) => {
      setToolbarPacks((current) => ({
        ...current,
        problems: [{ file: "toolbars", message: describeError(failure) }],
      }));
    });
  }

  // Removing the pack in use must not leave the View menu pointing at a bar
  // that is gone. The default bar takes over, along with its own layout.
  function forgetToolbarPack(id: string) {
    if (toolbarPreference.active !== id) {
      return;
    }
    const next = withActiveToolbar(toolbarPreference, DEFAULT_TOOLBAR_ID);
    setToolbarPreference(next);
    saveToolbarPreference(next);
  }

  async function refreshToolbarPacks(): Promise<ToolbarPacksSnapshot> {
    const snapshot = await listToolbarPacks();
    setToolbarPacks(snapshot);
    return snapshot;
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

  // Browsing from Help also moves the run on, so an open never repeats the
  // tip the reader just closed. The startup tip uses the same step.
  function openTips() {
    setTipIndex(nextTipIndex());
    setTipsOpen(true);
  }

  function changeTip(index: number) {
    setTipIndex(index);
    saveTipCursor(index);
  }

  // Permanent, unlike closing the popup. Settings > Tips turns it back on.
  function disableStartupTips() {
    saveSettings({ ...loadSettings(), tipsOnStartup: false });
    setTipsOpen(false);
  }

  // No box in between: the Help row and the toolbar button hand the invitation
  // straight to the share window of the system. Only a machine that can show
  // neither that window nor the Web Share API falls back to the clipboard, and
  // only a clipboard that refuses says anything.
  function tellFriend() {
    const system = isDesktop()
      ? () => shareToSystem(APP_NAME, APP_TAGLINE, APP_HOME_PAGE)
      : undefined;
    void shareApp(system).then((outcome) => {
      if (outcome === "failed") {
        downloads.reportError(
          "The invitation could not be shared or copied. The address is " +
            APP_HOME_PAGE,
        );
      }
    });
  }

  function handleCommand(command: ShellCommand) {
    if (command.startsWith("font:")) {
      chooseFont(command.slice("font:".length));
      return;
    }

    if (command.startsWith("toolbar-pack:")) {
      switchToolbarPack(command.slice("toolbar-pack:".length));
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
      case "tell-friend":
        tellFriend();
        break;
      case "tip-of-the-day":
        openTips();
        break;
      case "whats-new":
        whatsNew.openManually();
        break;
      case "website":
        // The window is a webview, so the address goes to the browser instead
        // of replacing the page that is running.
        if (isDesktop()) {
          void openUrl(APP_HOME_PAGE).catch((failure: unknown) =>
            downloads.reportError(describeError(failure)),
          );
        } else {
          window.open(APP_HOME_PAGE, "_blank", "noopener");
        }
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
      case "toolbar-customize":
        setCustomizeToolbarOpen(true);
        break;
      case "toolbar-new":
        openToolbarManager();
        break;
      case "tray-hidden":
        chooseTrayHidden(!tray.hidden);
        break;
      case "tray-dark-mode":
        chooseTrayDarkMode(!tray.darkMode);
        break;
      case "tray-customize-list":
        setUrlListOpen(true);
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
        menuChecks={toolbarChecks}
        toolbarSize={toolbarSize}
        toolbarLayout={toolbarLayout}
        toolbarPack={activeToolbar}
        toolbarShowLabels={showToolbarLabels}
        toolbars={toolbarMenu}
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
        onWhatsNew={whatsNew.openManually}
        toolbar={{
          pack: activeToolbar,
          packs: allToolbars,
          layout: toolbarLayout,
          size: toolbarSize,
          showLabels: showToolbarLabels,
          onSelectPack: switchToolbarPack,
          onSize: setToolbar,
          onShowLabels: setToolbarLabels,
          onCustomize: () => setCustomizeToolbarOpen(true),
          onManage: openToolbarManager,
        }}
        tray={{
          preferences: tray,
          onHidden: chooseTrayHidden,
          onDarkMode: chooseTrayDarkMode,
          onCustomizeUrlList: () => setUrlListOpen(true),
        }}
      />
      <AboutDialog
        open={aboutOpen}
        onClose={() => setAboutOpen(false)}
        onWhatsNew={whatsNew.openManually}
      />
      <WhatsNewDialog
        open={whatsNew.open}
        release={whatsNew.release}
        manual={whatsNew.manual}
        onClose={whatsNew.dismiss}
      />
      <TipOfTheDayDialog
        open={tipsOpen}
        index={tipIndex}
        onIndexChange={changeTip}
        onClose={() => setTipsOpen(false)}
        onDisable={disableStartupTips}
      />
      <UpdateDialog
        open={updateOpen && !tipsOpen}
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
      {customizeToolbarOpen ? (
        <CustomizeToolbarDialog
          pack={activeToolbar}
          layout={toolbarLayout}
          defaultLayout={defaultToolbarLayoutFor(activeToolbar)}
          onSave={saveToolbarLayout}
          onClose={() => setCustomizeToolbarOpen(false)}
        />
      ) : null}
      {toolbarsOpen ? (
        <ToolbarsDialog
          snapshot={toolbarPacks}
          activeId={activeToolbar.id}
          onRefresh={refreshToolbarPacks}
          onPackRemoved={forgetToolbarPack}
          onClose={() => setToolbarsOpen(false)}
        />
      ) : null}
      {urlListOpen ? (
        <UrlListDialog
          protocols={tray.urlProtocols}
          onSave={saveUrlProtocols}
          onClose={() => setUrlListOpen(false)}
        />
      ) : null}
    </div>
  );
}
