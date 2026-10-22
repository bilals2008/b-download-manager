import { MenuPopup, type MenuRow } from "@/components/shell/menu-popup";
import { describeError } from "@/lib/errors";
import { splitPath, type Download } from "@/lib/downloads";
import { openPath } from "@/lib/ipc";
import { useDownloads } from "@/state/download-store";

/** What the menu of a download can do. */
type RowCommand =
  | "resume"
  | "pause"
  | "stop"
  | "redownload"
  | "delete"
  | "open-file"
  | "open-folder"
  | "properties";

interface DownloadContextMenuProps {
  item: Download;
  /** Where the pointer was, in window coordinates. */
  at: { x: number; y: number };
  onClose: () => void;
}

/**
 * The menu of a download row, the way IDM opens it on a right click.
 *
 * The commands work on the selection, and a right click puts the row under
 * the pointer into the selection first, so the menu always acts on what the
 * user sees.
 */
export function DownloadContextMenu({
  item,
  at,
  onClose,
}: DownloadContextMenuProps) {
  const { applyToSelection, redownloadSelected, reportError } = useDownloads();

  function open(target: string) {
    openPath(target).catch((failure: unknown) => {
      reportError(describeError(failure));
    });
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
      case "delete":
        applyToSelection("delete");
        break;
      case "open-file":
        open(item.dest);
        break;
      case "open-folder":
        open(splitPath(item.dest).dir);
        break;
      case "properties":
        // There is no properties window yet.
        break;
    }
    onClose();
  }

  return (
    <MenuPopup
      id="download-context-menu"
      label={`Actions for ${item.fileName}`}
      rows={rowsFor(item)}
      level={0}
      focus="first"
      at={at}
      onChoose={(row) => choose(row.command)}
      onClose={onClose}
    />
  );
}

/** The entries of the menu, in the order IDM lists them. */
function rowsFor(item: Download): MenuRow<RowCommand>[] {
  const finished = item.status === "completed";
  const running = item.status === "downloading";

  return [
    { command: "resume", label: "Resume", disabled: finished },
    { command: "pause", label: "Pause", disabled: !running },
    {
      command: "stop",
      label: "Stop",
      disabled: !running && item.status !== "paused",
    },
    { command: "redownload", label: "Redownload" },
    { command: "delete", label: "Remove" },
    "separator",
    { command: "open-file", label: "Open file", disabled: !finished },
    { command: "open-folder", label: "Open containing folder" },
    "separator",
    { command: "properties", label: "Properties...", disabled: true },
  ];
}
