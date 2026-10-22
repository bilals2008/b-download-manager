import {
  CopyGlyph,
  FolderGlyph,
  InfoGlyph,
  OpenFileGlyph,
  PauseGlyph,
  PlayGlyph,
  RefreshGlyph,
  StopGlyph,
  TrashGlyph,
} from "@/components/icons/action-icons";
import { MenuPopup, type MenuRow } from "@/components/shell/menu-popup";
import { copyText } from "@/lib/clipboard";
import { splitPath, type Download } from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { openPath } from "@/lib/ipc";
import { useDownloads } from "@/state/download-store";

/** What the menu of a download can do. */
type RowCommand =
  | "resume"
  | "pause"
  | "stop"
  | "redownload"
  | "remove"
  | "open-file"
  | "open-folder"
  | "copy-address"
  | "properties";

interface DownloadContextMenuProps {
  item: Download;
  /** Where the pointer was, in window coordinates. */
  at: { x: number; y: number };
  /** Open the properties window of the row the menu was opened on. */
  onShowProperties: (item: Download) => void;
  onClose: () => void;
}

/**
 * The menu of a download row, the way IDM opens it on a right click.
 *
 * The commands work on the selection, and a right click puts the row under
 * the pointer into the selection first, so the menu always acts on what the
 * user sees. Entries that cannot do anything to the current status are drawn
 * but inert, which is how a menu teaches what a row can do.
 */
export function DownloadContextMenu({
  item,
  at,
  onShowProperties,
  onClose,
}: DownloadContextMenuProps) {
  const {
    downloads,
    selected,
    applyToSelection,
    redownloadSelected,
    reportError,
  } = useDownloads();
  // A right click adds its row to the selection first, so the menu acts on
  // every row the user sees picked.
  const targets = downloads.filter((download) =>
    selected.includes(download.id),
  );
  const items = targets.length > 0 ? targets : [item];
  const count = items.length;

  function open(target: string) {
    openPath(target).catch((failure: unknown) => {
      reportError(describeError(failure));
    });
  }

  async function copyAddress() {
    const addresses = items.map((download) => download.url).join("\n");
    if (!(await copyText(addresses))) {
      reportError("The address could not be copied to the clipboard.");
    }
  }

  function choose(command: RowCommand) {
    switch (command) {
      case "resume":
        applyToSelection("resume");
        break;
      case "pause":
        applyToSelection("pause");
        break;
      case "stop":
        applyToSelection("stop");
        break;
      case "redownload":
        redownloadSelected();
        break;
      case "remove":
        applyToSelection("delete");
        break;
      case "open-file":
        open(item.dest);
        break;
      case "open-folder":
        open(splitPath(item.dest).dir);
        break;
      case "copy-address":
        void copyAddress();
        break;
      case "properties":
        onShowProperties(item);
        break;
    }
    onClose();
  }

  return (
    <MenuPopup
      id="download-context-menu"
      label={`Actions for ${item.fileName}`}
      rows={rowsFor(items)}
      menuLabels={labelsFor(count)}
      level={0}
      focus="first"
      at={at}
      onChoose={(row) => choose(row.command)}
      onClose={onClose}
    />
  );
}

/** The entries of the menu, in the order IDM lists them. */
function rowsFor(items: Download[]): MenuRow<RowCommand>[] {
  // One row can be opened or shown in its folder; a handful of them cannot.
  const single = items.length === 1 ? items[0] : null;
  const anyLive = items.some(
    (item) => item.status === "downloading" || item.status === "queued",
  );
  const anyStoppable = items.some(
    (item) =>
      item.status === "downloading" ||
      item.status === "queued" ||
      item.status === "paused",
  );
  const canResume = items.some(
    (item) => item.status !== "completed" && item.status !== "downloading",
  );

  return [
    {
      command: "resume",
      label: "Resume",
      icon: <PlayGlyph className="size-3.5" />,
      disabled: !canResume,
    },
    {
      command: "pause",
      label: "Pause",
      icon: <PauseGlyph className="size-3.5" />,
      disabled: !anyLive,
    },
    {
      command: "stop",
      label: "Stop",
      icon: <StopGlyph className="size-3.5" />,
      disabled: !anyStoppable,
    },
    {
      command: "redownload",
      label: "Redownload",
      icon: <RefreshGlyph className="size-3.5" />,
    },
    {
      command: "remove",
      label: "Remove",
      icon: <TrashGlyph className="size-3.5" />,
      danger: true,
    },
    "separator",
    {
      command: "open-file",
      label: "Open file",
      icon: <OpenFileGlyph className="size-3.5" />,
      disabled: single?.status !== "completed",
    },
    {
      command: "open-folder",
      label: "Open containing folder",
      icon: <FolderGlyph className="size-3.5" />,
      disabled: single === null,
    },
    {
      command: "copy-address",
      label: "Copy address",
      icon: <CopyGlyph className="size-3.5" />,
    },
    "separator",
    {
      command: "properties",
      label: "Properties...",
      icon: <InfoGlyph className="size-3.5" />,
    },
  ];
}

/** The entries that name how many rows they act on. */
function labelsFor(count: number): Partial<Record<RowCommand, string>> {
  if (count < 2) {
    return {};
  }
  return {
    resume: `Resume ${count} downloads`,
    pause: `Pause ${count} downloads`,
    stop: `Stop ${count} downloads`,
    redownload: `Redownload ${count} downloads`,
    remove: `Remove ${count} downloads`,
    "copy-address": `Copy ${count} addresses`,
  };
}
