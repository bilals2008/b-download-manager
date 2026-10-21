import { useCallback, useRef, useState } from "react";

import type { DownloadColumn } from "@/components/workspace/download-columns";

export type ColumnWidths = Record<string, number>;

interface DragState {
  index: number;
  startX: number;
  tableWidth: number;
  startWidths: ColumnWidths;
}

function defaults(columns: DownloadColumn[]): ColumnWidths {
  return Object.fromEntries(columns.map((column) => [column.id, column.width]));
}

/**
 * Column resizing for the download table. A drag takes width from the next
 * column and gives it to the one being dragged, so the widths always add up to
 * 100 percent and the table never leaves a gap.
 */
export function useColumnWidths(columns: DownloadColumn[]) {
  const [widths, setWidths] = useState<ColumnWidths>(() => defaults(columns));
  const tableRef = useRef<HTMLTableElement>(null);
  const drag = useRef<DragState | null>(null);

  const pair = useCallback(
    (base: ColumnWidths, index: number, delta: number) => {
      const column = columns[index];
      const neighbourIndex = index < columns.length - 1 ? index + 1 : index - 1;
      const neighbour = columns[neighbourIndex];
      if (!neighbour) {
        return {};
      }
      const own = base[column.id] ?? column.width;
      const other = base[neighbour.id] ?? neighbour.width;
      const combined = own + other;
      const next = Math.min(
        Math.max(own + delta, column.minWidth),
        Math.max(column.minWidth, combined - neighbour.minWidth),
      );
      return { [column.id]: next, [neighbour.id]: combined - next };
    },
    [columns],
  );

  const beginResize = useCallback(
    (index: number, clientX: number) => {
      drag.current = {
        index,
        startX: clientX,
        tableWidth: tableRef.current?.getBoundingClientRect().width || 1,
        startWidths: widths,
      };
    },
    [widths],
  );

  const moveResize = useCallback(
    (clientX: number) => {
      const current = drag.current;
      if (!current) {
        return;
      }
      const delta =
        ((clientX - current.startX) / current.tableWidth) * 100;
      const patch = pair(current.startWidths, current.index, delta);
      setWidths((value) => ({ ...value, ...patch }));
    },
    [pair],
  );

  const endResize = useCallback(() => {
    drag.current = null;
  }, []);

  const resizeBy = useCallback(
    (index: number, delta: number) => {
      setWidths((value) => ({ ...value, ...pair(value, index, delta) }));
    },
    [pair],
  );

  const resetWidths = useCallback(() => {
    setWidths(defaults(columns));
  }, [columns]);

  return {
    widths,
    tableRef,
    beginResize,
    moveResize,
    endResize,
    resizeBy,
    resetWidths,
  };
}
