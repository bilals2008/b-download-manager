import {
  Fragment,
  useId,
  useMemo,
  useState,
  type ChangeEvent,
  type ReactNode,
} from "react";

import {
  CheckGlyph,
  ChevronDown,
  ChevronRight,
  Glyph,
} from "@/components/icons/ui-icons";
import { formatSize } from "@/lib/downloads";
import { cn } from "@/lib/utils";

/**
 * The part of a job file this tree reads: where it goes and how big it is.
 *
 * The tree is drawn before a job exists — the add dialog has only what the
 * torrent file said — so it asks for those two fields and not for the
 * progress a running job has.
 */
export interface TorrentFileEntry {
  /** The file's path inside the torrent. */
  path: string;
  /** How many bytes it holds. */
  length: number;
}

interface TorrentFileTreeProps {
  /** The files of the torrent, in the order the torrent holds them. */
  files: readonly TorrentFileEntry[];
  /** The indexes the reader kept. */
  selected: readonly number[];
  onChange: (selected: number[]) => void;
  disabled?: boolean;
}

/** One folder of the torrent and the files in it. */
interface Group {
  /** The folder's path inside the torrent; the root has none. */
  folder: string;
  /** The indexes of the files in it. */
  indexes: number[];
  /** How many bytes they hold together. */
  length: number;
}

/** The native box of a row, with only its face replaced. */
const CHECK_BOX =
  "peer size-4 shrink-0 appearance-none rounded-sm border border-input bg-chrome-canvas transition-colors hover:border-chrome-muted checked:border-primary checked:bg-primary disabled:border-chrome-line disabled:bg-chrome-panel disabled:checked:bg-chrome-line/60";

/** A closed folder, in the same single stroke as the other chrome glyphs. */
export function FolderGlyph({ className }: { className?: string }) {
  return (
    <Glyph className={className}>
      <path d="M2.2 4.6A1.4 1.4 0 0 1 3.6 3.2h2.8l1.2 1.4h5a1.4 1.4 0 0 1 1.4 1.4v5.4a1.4 1.4 0 0 1-1.4 1.4H3.6a1.4 1.4 0 0 1-1.4-1.4z" />
    </Glyph>
  );
}

/** A sheet of paper with its corner folded. */
function FileGlyph({ className }: { className?: string }) {
  return (
    <Glyph className={className}>
      <path d="M4 2.4h5l3 3v8.2H4z" />
      <path d="M9 2.4v3h3" />
    </Glyph>
  );
}

interface RowCheckboxProps {
  id: string;
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (event: ChangeEvent<HTMLInputElement>) => void;
  icon?: ReactNode;
  className?: string;
}

/** A row's checkbox: a native input and its label, with a glyph before the name. */
function RowCheckbox({
  id,
  label,
  checked,
  disabled,
  onChange,
  icon,
  className,
}: RowCheckboxProps) {
  return (
    <label
      htmlFor={id}
      className={cn(
        "flex min-w-0 flex-1 cursor-pointer items-center gap-2 py-0.5",
        disabled && "cursor-default opacity-55",
        className,
      )}
    >
      <span className="relative flex size-4 shrink-0 items-center justify-center">
        <input
          id={id}
          type="checkbox"
          checked={checked}
          disabled={disabled}
          onChange={onChange}
          className={CHECK_BOX}
        />
        <CheckGlyph className="pointer-events-none absolute size-3 text-primary-foreground opacity-0 peer-checked:opacity-100 peer-disabled:text-chrome-muted" />
      </span>
      {icon}
      <span className="min-w-0 truncate text-xs text-chrome-menu-text">
        {label}
      </span>
    </label>
  );
}

/**
 * The files of a torrent, grouped the way the torrent is written.
 *
 * A folder is ticked or unticked as a whole and the files under it follow; a
 * folder can also be folded away without changing what is selected. The tree
 * scrolls on its own, so a torrent of hundreds of files never pushes the Save
 * to row out of the dialog.
 */
export function TorrentFileTree({
  files,
  selected,
  onChange,
  disabled,
}: TorrentFileTreeProps) {
  const picked = useMemo(() => new Set(selected), [selected]);
  const groups = useMemo(() => groupFiles(files), [files]);
  const bytes = files.reduce(
    (sum, file, index) => (picked.has(index) ? sum + file.length : sum),
    0,
  );
  const idPrefix = useId();
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set(),
  );

  function setIndexes(indexes: number[], on: boolean) {
    const next = new Set(selected);
    for (const index of indexes) {
      if (on) {
        next.add(index);
      } else {
        next.delete(index);
      }
    }
    onChange([...next].sort((a, b) => a - b));
  }

  function toggleFolder(folder: string) {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(folder)) {
        next.delete(folder);
      } else {
        next.add(folder);
      }
      return next;
    });
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="max-h-64 overflow-y-auto rounded-chrome bg-chrome-canvas scrollbar-thin">
        <table
          className="w-full table-fixed border-collapse text-xs"
          aria-label="Files in this torrent"
        >
          <thead>
            <tr>
              <th
                scope="col"
                className="sticky top-0 z-10 bg-chrome-canvas px-3 py-2 text-left text-chrome-micro font-medium text-chrome-muted"
              >
                Name
              </th>
              <th
                scope="col"
                className="sticky top-0 z-10 w-24 bg-chrome-canvas px-3 py-2 text-right text-chrome-micro font-medium text-chrome-muted"
              >
                Size
              </th>
            </tr>
          </thead>
          <tbody>
            {groups.map((group, groupIndex) => {
              const kept = group.indexes.filter((index) =>
                picked.has(index),
              ).length;
              const all = kept === group.indexes.length;
              const folded = collapsed.has(group.folder);
              return (
                <Fragment key={group.folder || groupIndex}>
                  {group.folder ? (
                    <tr className="hover:bg-chrome-popup-hover">
                      <td className="px-2 py-1">
                        <div className="flex min-w-0 items-center gap-1">
                          <button
                            type="button"
                            aria-expanded={!folded}
                            aria-label={`${folded ? "Expand" : "Collapse"} ${group.folder}`}
                            onClick={() => toggleFolder(group.folder)}
                            className="flex size-5 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
                          >
                            {folded ? (
                              <ChevronRight className="size-3.5" />
                            ) : (
                              <ChevronDown className="size-3.5" />
                            )}
                          </button>
                          <RowCheckbox
                            id={`${idPrefix}-folder-${groupIndex}`}
                            label={group.folder}
                            checked={all}
                            disabled={disabled}
                            onChange={(event) =>
                              setIndexes(group.indexes, event.target.checked)
                            }
                            icon={
                              <FolderGlyph className="size-3.5 shrink-0 text-chrome-muted" />
                            }
                            className="font-medium"
                          />
                        </div>
                      </td>
                      <td className="px-3 py-1 text-right text-chrome-micro tabular-nums text-chrome-muted">
                        {formatSize(group.length)}
                      </td>
                    </tr>
                  ) : null}
                  {folded && group.folder
                    ? null
                    : group.indexes.map((index) => (
                        <tr
                          key={`${index}-${files[index].path}`}
                          className="hover:bg-chrome-popup-hover"
                        >
                          <td className="py-1 pr-3 pl-9">
                            <RowCheckbox
                              id={`${idPrefix}-file-${index}`}
                              label={fileName(files[index].path)}
                              checked={picked.has(index)}
                              disabled={disabled}
                              onChange={(event) =>
                                setIndexes([index], event.target.checked)
                              }
                              icon={
                                <FileGlyph className="size-3.5 shrink-0 text-chrome-muted" />
                              }
                            />
                          </td>
                          <td className="px-3 py-1 text-right text-chrome-micro tabular-nums text-chrome-muted">
                            {formatSize(files[index].length)}
                          </td>
                        </tr>
                      ))}
                </Fragment>
              );
            })}
          </tbody>
        </table>
      </div>
      <div className="flex items-center justify-between gap-2">
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            disabled={disabled || selected.length === files.length}
            onClick={() => onChange(files.map((_, index) => index))}
            className="rounded-chrome px-1.5 py-0.5 text-chrome-micro text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none disabled:cursor-default disabled:opacity-45 disabled:hover:bg-transparent disabled:hover:text-chrome-muted"
          >
            Select all
          </button>
          <button
            type="button"
            disabled={disabled || selected.length === 0}
            onClick={() => onChange([])}
            className="rounded-chrome px-1.5 py-0.5 text-chrome-micro text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none disabled:cursor-default disabled:opacity-45 disabled:hover:bg-transparent disabled:hover:text-chrome-muted"
          >
            Select none
          </button>
        </div>
        <p className="min-w-0 truncate text-chrome-micro tabular-nums text-chrome-muted">
          {selected.length} of {files.length}{" "}
          {files.length === 1 ? "file" : "files"} · {formatSize(bytes)}
        </p>
      </div>
    </div>
  );
}

/** The name of a file without the folders it sits in. */
function fileName(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? path : path.slice(at + 1);
}

/** The files of a torrent, grouped by the folder they sit in. */
function groupFiles(files: readonly TorrentFileEntry[]): Group[] {
  const groups = new Map<string, Group>();
  files.forEach((file, index) => {
    const at = file.path.lastIndexOf("/");
    const folder = at < 0 ? "" : file.path.slice(0, at);
    const group = groups.get(folder) ?? { folder, indexes: [], length: 0 };
    group.indexes.push(index);
    group.length += file.length;
    groups.set(folder, group);
  });
  return [...groups.values()];
}
