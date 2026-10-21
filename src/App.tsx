import { useState } from "react";

import { SettingsDialog } from "@/components/settings/settings-dialog";
import type { ShellCommand } from "@/components/shell/commands";
import { TopBar } from "@/components/shell/top-bar";
import { AboutDialog } from "@/components/workspace/about-dialog";
import { AddDownloadDialog } from "@/components/workspace/add-download-dialog";
import { Workspace } from "@/components/workspace/workspace";
import { windowAction } from "@/lib/window";
import { useDownloads } from "@/state/download-store";

export default function App() {
  const downloads = useDownloads();
  const [categoriesVisible, setCategoriesVisible] = useState(true);
  const [addDownloadOpen, setAddDownloadOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);

  function handleCommand(command: ShellCommand) {
    switch (command) {
      case "exit":
        void windowAction("close");
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
    }
  }

  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-chrome-canvas text-chrome-toolbar-text">
      <TopBar
        onCommand={handleCommand}
        menuLabels={{
          categories: categoriesVisible ? "Hide categories" : "Show categories",
        }}
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
