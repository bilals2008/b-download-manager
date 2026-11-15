import { Checkbox } from "@/components/ui/checkbox";
import { DialogAction } from "@/components/ui/dialog-action";
import { formatSize } from "@/lib/downloads";
import type { TorrentFile } from "@/lib/ipc-torrent";

interface TorrentFileListProps {
  /** The files of the torrent, in the order the torrent holds them. */
  files: readonly TorrentFile[];
  /** The indexes the reader kept. */
  selected: readonly number[];
  onChange: (selected: number[]) => void;
  disabled?: boolean;
}

/**
 * The files of a torrent, each one a tick: what the reader leaves ticked is
 * what the job downloads. A torrent can hold hundreds of files, so the list
 * scrolls inside the dialog and the count above it follows the ticks.
 */
export function TorrentFileList({
  files,
  selected,
  onChange,
  disabled,
}: TorrentFileListProps) {
  const picked = new Set(selected);
  const bytes = files.reduce(
    (sum, file, index) => (picked.has(index) ? sum + file.length : sum),
    0,
  );

  function toggle(index: number, on: boolean) {
    if (on) {
      onChange([...selected, index].sort((a, b) => a - b));
    } else {
      onChange(selected.filter((entry) => entry !== index));
    }
  }

  return (
    <div className="flex flex-col">
      <div className="flex items-center justify-between gap-2 pb-1">
        <p className="text-chrome-micro text-chrome-muted">
          {selected.length} of {files.length} files · {formatSize(bytes)}
        </p>
        <div className="flex shrink-0 items-center gap-1">
          <DialogAction
            tone="quiet"
            disabled={disabled || selected.length === files.length}
            onClick={() => onChange(files.map((_, index) => index))}
          >
            Select all
          </DialogAction>
          <DialogAction
            tone="quiet"
            disabled={disabled || selected.length === 0}
            onClick={() => onChange([])}
          >
            Select none
          </DialogAction>
        </div>
      </div>
      <ul className="max-h-56 overflow-y-auto scrollbar-thin">
        {files.map((file, index) => (
          <li key={`${index}-${file.path}`}>
            <Checkbox
              label={file.path}
              hint={formatSize(file.length)}
              checked={picked.has(index)}
              disabled={disabled}
              onChange={(event) => toggle(index, event.target.checked)}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}
