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
import { WarningMark, CategoryFolderIcon } from "@/components/icons/desktop-icons";
import { CloseGlyph, DownloadGlyph, PlusGlyph } from "@/components/icons/ui-icons";
import { MenuPopup, type MenuRow } from "@/components/shell/menu-popup";
import { copyText } from "@/lib/clipboard";
import {
  canDeleteFile,
  splitPath,
  type CategoryDefinition,
  type Download,
} from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { openPath, openProgressWindow } from "@/lib/ipc";
import { useDownloads } from "@/state/download-store";

/** What the menu of a download can do. */
type RowCommand =
  | "resume"
  | "pause"
  | "stop"
  | "queue"
  | "dequeue"
  | "recover"
  | "redownload"
  | "remove"
  | "delete-file"
  | "open-file"
  | "open-folder"
  | "open-window"
  | "copy-address"
  | "copy-path"
  | "category"
  | `category:${string}`
  | "properties";

interface DownloadContextMenuProps {
  item: Download;
  /** Where the pointer was, in window coordinates. */
  at: { x: number; y: number };
  /** Open the properties window of the row the menu was opened on. */
  onShowProperties: (item: Download) => void;
  /** Open the failure window of a row that could not finish. */
  onShowFailure: (item: Download) => void;
  /** Ask before deleting the file of a finished row from the disk. */
  onDeleteFile: (item: Download) => void;
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
  onShowFailure,
  onDeleteFile,
  onClose,
}: DownloadContextMenuProps) {
  const {
    downloads,
    categories,
    selected,
    applyToSelection,
    assignCategory,
    dequeue,
    queue,
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
  // The category the whole selection shares, or null when it is mixed.
  const sharedCategory =
    items.every((download) => download.category === items[0].category)
      ? items[0].category
      : null;

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

  async function copyPath() {
    const paths = items.map((download) => download.dest).join("\n");
    if (!(await copyText(paths))) {
      reportError("The file path could not be copied to the clipboard.");
    }
  }

  function choose(command: RowCommand) {
    if (command.startsWith("category:")) {
      assignCategory(
        items.map((download) => download.id),
        command.slice("category:".length),
      );
      onClose();
      return;
    }
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
      case "queue":
        queue(items.map((download) => download.id));
        break;
      case "dequeue":
        dequeue(items.map((download) => download.id));
        break;
      case "recover":
        onShowFailure(item);
        break;
      case "redownload":
        redownloadSelected();
        break;
      case "remove":
        applyToSelection("delete");
        break;
      case "delete-file":
        onDeleteFile(item);
        break;
      case "open-file":
        open(item.dest);
        break;
      case "open-folder":
        open(splitPath(item.dest).dir);
        break;
      case "open-window":
        openProgressWindow(item.id).catch((failure: unknown) => {
          reportError(describeError(failure));
        });
        break;
      case "copy-address":
        void copyAddress();
        break;
      case "copy-path":
        void copyPath();
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
      rows={rowsFor(items, categories, sharedCategory)}
      menuLabels={labelsFor(count)}
      checked={
        sharedCategory ? { [`category:${sharedCategory}`]: true } : undefined
      }
      level={0}
      focus="first"
      at={at}
      onChoose={(row) => choose(row.command)}
      onClose={onClose}
    />
  );
}

/** The entries of the menu, in the order IDM lists them. */
function rowsFor(
  items: Download[],
  categories: CategoryDefinition[],
  sharedCategory: string | null,
): MenuRow<RowCommand>[] {
  // One row can be opened or shown in its folder; a handful of them cannot.
  const single = items.length === 1 ? items[0] : null;
  const anyDownloading = items.some((item) => item.status === "downloading");
  const anyStoppable = items.some(
    (item) =>
      item.status === "downloading" ||
      item.status === "queued" ||
      item.status === "paused",
  );
  // A queued row is waiting already; a finished one has nothing to wait for.
  const canQueue = items.some(
    (item) => item.status !== "completed" && item.status !== "queued",
  );
  const canDequeue = items.some((item) => item.status === "queued");
  const canResume = items.some(
    (item) =>
      item.status === "paused" ||
      item.status === "stopped" ||
      item.status === "error",
  );
  // Deleting the file is the one command that loses something on the disk, so
  // it is offered for one finished row and never for a batch.
  const canDelete = single !== null && canDeleteFile(single);

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
      disabled: !anyDownloading,
    },
    {
      command: "stop",
      label: "Stop",
      icon: <StopGlyph className="size-3.5" />,
      disabled: !anyStoppable,
    },
    {
      command: "queue",
      label: "Add to queue",
      icon: <PlusGlyph className="size-3.5" />,
      disabled: !canQueue,
    },
    {
      command: "dequeue",
      label: "Remove from queue",
      icon: <PauseGlyph className="size-3.5" />,
      disabled: !canDequeue,
    },
    {
      command: "recover",
      label: "Show error and retry...",
      icon: <WarningMark tone="red" className="size-3.5" />,
      disabled: single?.status !== "error",
    },
    {
      command: "redownload",
      label: "Redownload",
      icon: <RefreshGlyph className="size-3.5" />,
      // A running row has to be stopped first; the engine refuses to replace
      // a row it is still writing.
      disabled: items.some(
        (item) => item.status === "downloading" || item.status === "queued",
      ),
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
      command: "open-window",
      label: "Download progress window",
      icon: <DownloadGlyph className="size-3.5" />,
      disabled: single === null,
    },
    "separator",
    {
      command: "copy-address",
      label: "Copy address",
      icon: <CopyGlyph className="size-3.5" />,
    },
    {
      command: "copy-path",
      label: "Copy file path",
      icon: <CopyGlyph className="size-3.5" />,
    },
    "separator",
    {
      command: "category",
      label: "Category",
      icon: (
        <CategoryFolderIcon
          category={sharedCategory ?? "Other"}
          className="size-3.5"
        />
      ),
      submenu: categories.map((category) => ({
        command: `category:${category.name}`,
        label: category.name,
        radio: true,
        icon: (
          <CategoryFolderIcon
            category={category.name}
            className="size-3.5"
          />
        ),
      })),
    },
    {
      command: "properties",
      label: "Properties...",
      icon: <InfoGlyph className="size-3.5" />,
    },
    "separator",
    {
      command: "remove",
      label: "Remove from list",
      icon: <CloseGlyph className="size-3.5" />,
    },
    {
      command: "delete-file",
      label: "Delete file...",
      icon: <TrashGlyph className="size-3.5" />,
      danger: true,
      disabled: !canDelete,
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
    queue: `Add ${count} to queue`,
    dequeue: `Remove ${count} from queue`,
    redownload: `Redownload ${count} downloads`,
    remove: `Remove ${count} from list`,
    "copy-address": `Copy ${count} addresses`,
    "copy-path": `Copy ${count} file paths`,
  };
}
