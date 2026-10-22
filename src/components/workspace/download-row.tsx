import { useRef, type ComponentType } from "react";
import {
  AlertTriangle,
  ArrowDownToLine,
  CheckCircle2,
  Clock,
  Pause,
  Square,
  type LucideProps,
} from "lucide-react";

import { CATEGORY_ICONS } from "@/components/workspace/category-icons";
import {
  formatEta,
  formatSize,
  formatSpeed,
  progressOf,
  STATUS_LABELS,
  type Download,
} from "@/lib/downloads";
import { cn } from "@/lib/utils";

interface StatusStyle {
  icon: ComponentType<LucideProps>;
  tint: string;
}

const STATUS_STYLE: Record<Download["status"], StatusStyle> = {
  downloading: { icon: ArrowDownToLine, tint: "text-chrome-focus" },
  queued: { icon: Clock, tint: "text-icon-amber" },
  paused: { icon: Pause, tint: "text-icon-blue-bright" },
  stopped: { icon: Square, tint: "text-chrome-muted" },
  completed: { icon: CheckCircle2, tint: "text-icon-green" },
  error: { icon: AlertTriangle, tint: "text-icon-red-bright" },
};

interface DownloadRowProps {
  item: Download;
  selected: boolean;
  onSelect: (additive: boolean) => void;
  /** Open the menu of the row, the way a right click does. */
  onOpenMenu: (at: { x: number; y: number }, trigger: HTMLElement | null) => void;
}

/**
 * One download, the way IDM shows it: the name, the size, the progress as a
 * percentage, the speed, the time left and the status. Nothing opens under
 * the row, so the list stays a list.
 */
export function DownloadRow({
  item,
  selected,
  onSelect,
  onOpenMenu,
}: DownloadRowProps) {
  const nameRef = useRef<HTMLButtonElement>(null);
  const fileType = CATEGORY_ICONS[item.category];
  const FileIcon = fileType.icon;
  const status = STATUS_STYLE[item.status];
  const StatusIcon = status.icon;
  const percent = progressOf(item);
  const knownSize = item.size > 0;

  /** The menu of a row opens at the pointer, or at the row for the keyboard. */
  function openMenu(at?: { x: number; y: number }) {
    const box = nameRef.current?.getBoundingClientRect();
    onOpenMenu(
      at ?? { x: box?.left ?? 0, y: box?.bottom ?? 0 },
      nameRef.current,
    );
  }

  return (
    <tr
      aria-selected={selected}
      onClick={(event) => onSelect(event.ctrlKey || event.metaKey)}
      onContextMenu={(event) => {
        event.preventDefault();
        openMenu({ x: event.clientX, y: event.clientY });
      }}
      onKeyDown={(event) => {
        if (
          event.key === "ContextMenu" ||
          (event.shiftKey && event.key === "F10")
        ) {
          event.preventDefault();
          openMenu();
        }
      }}
      className={cn(
        "cursor-default border-b border-chrome-line/60 transition-colors",
        selected
          ? "bg-chrome-menu-active"
          : "hover:bg-chrome-menu-hover",
      )}
    >
      <td className="border-b border-chrome-line/60 px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className="grid size-6 shrink-0 place-items-center rounded-chrome bg-chrome-popup"
            aria-hidden="true"
          >
            <FileIcon className={cn("size-3.5", fileType.tint)} strokeWidth={1.75} />
          </span>

          <button
            ref={nameRef}
            type="button"
            title={item.url}
            onClick={(event) => {
              event.stopPropagation();
              onSelect(event.ctrlKey || event.metaKey);
            }}
            className="min-w-0 flex-1 text-left focus-visible:rounded-chrome focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <span className="block truncate text-chrome-menu-text">
              {item.fileName}
            </span>
            <span
              className={cn(
                "mt-0.5 block truncate text-chrome-micro",
                item.error ? "text-icon-red-bright" : "text-chrome-muted",
              )}
            >
              {item.error ?? item.savePath}
            </span>
          </button>
        </div>
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-2 text-right tabular-nums text-chrome-menu-text">
        {knownSize ? formatSize(item.size) : "-"}
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-2 text-right tabular-nums text-chrome-menu-text">
        {knownSize ? `${percent.toFixed(2)}%` : "-"}
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-2 text-right tabular-nums text-chrome-menu-text">
        {formatSpeed(item.speed) || "-"}
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-2 text-right tabular-nums text-chrome-menu-text">
        {item.status === "downloading" ? formatEta(item.eta) : "-"}
      </td>

      <td className="border-b border-chrome-line/60 px-3 py-2">
        <span
          className="inline-flex items-center gap-1.5 text-chrome-menu-text"
          title={item.error ?? STATUS_LABELS[item.status]}
        >
          <StatusIcon
            className={cn("size-3.5 shrink-0", status.tint)}
            aria-hidden="true"
          />
          {STATUS_LABELS[item.status]}
        </span>
      </td>
    </tr>
  );
}
