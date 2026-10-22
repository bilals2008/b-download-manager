import type { SortKey } from "@/lib/downloads";

export interface DownloadColumn {
  id: string;
  label: string;
  sortKey: SortKey | null;
  /** Share of the table width when the app starts, in percent. */
  width: number;
  /** Smallest share the column may be dragged to, in percent. */
  minWidth: number;
  align?: "right";
}

/**
 * The six IDM columns, with a sensible starting width. Widths are shares of the
 * table width so the table keeps filling the window at every size.
 */
export const DOWNLOAD_COLUMNS: DownloadColumn[] = [
  { id: "name", label: "File name", sortKey: "name", width: 34, minWidth: 18 },
  {
    id: "size",
    label: "Size",
    sortKey: "size",
    width: 10,
    minWidth: 7,
    align: "right",
  },
  {
    id: "progress",
    label: "Progress",
    sortKey: null,
    width: 9,
    minWidth: 6,
    align: "right",
  },
  {
    id: "speed",
    label: "Speed",
    sortKey: "speed",
    width: 11,
    minWidth: 7,
    align: "right",
  },
  {
    id: "eta",
    label: "Time left",
    sortKey: "eta",
    width: 10,
    minWidth: 7,
    align: "right",
  },
  { id: "status", label: "Status", sortKey: "status", width: 19, minWidth: 10 },
];
