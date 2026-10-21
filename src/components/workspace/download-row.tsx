import {
  useEffect,
  useId,
  useState,
  type ComponentType,
  type ReactNode,
} from "react";
import {
  AlertTriangle,
  ArrowDownToLine,
  Check,
  CheckCircle2,
  ChevronRight,
  Clock,
  Copy,
  Pause,
  Square,
  type LucideProps,
} from "lucide-react";

import { CATEGORY_ICONS } from "@/components/workspace/category-icons";
import {
  formatDate,
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

const BAR_TINT: Record<Download["status"], string> = {
  downloading: "bg-chrome-focus",
  queued: "bg-chrome-muted",
  paused: "bg-icon-blue-bright",
  stopped: "bg-chrome-muted",
  completed: "bg-icon-green",
  error: "bg-icon-red-bright",
};

interface DownloadRowProps {
  item: Download;
  selected: boolean;
  expanded: boolean;
  columnCount: number;
  onSelect: (additive: boolean) => void;
  onToggleExpand: () => void;
}

/**
 * One download, plus its details row when expanded. The row itself is the
 * selection target; the chevron is a separate control so a keyboard user can
 * open the details without changing the selection.
 */
export function DownloadRow({
  item,
  selected,
  expanded,
  columnCount,
  onSelect,
  onToggleExpand,
}: DownloadRowProps) {
  const detailsId = useId();
  const fileType = CATEGORY_ICONS[item.category];
  const FileIcon = fileType.icon;
  const status = STATUS_STYLE[item.status];
  const StatusIcon = status.icon;
  const percent = progressOf(item);
  const knownSize = item.size > 0;

  return (
    <>
      <tr
        aria-selected={selected}
        onClick={(event) => onSelect(event.ctrlKey || event.metaKey)}
        className={cn(
          "cursor-default border-b border-chrome-line/60 transition-colors",
          selected
            ? "bg-chrome-menu-active"
            : "hover:bg-chrome-menu-hover",
        )}
      >
        <td className="border-b border-chrome-line/60 px-3 py-2">
          <div className="flex min-w-0 items-center gap-2">
            <button
              type="button"
              aria-expanded={expanded}
              aria-controls={detailsId}
              aria-label={`${expanded ? "Hide" : "Show"} details for ${item.fileName}`}
              onClick={(event) => {
                event.stopPropagation();
                onToggleExpand();
              }}
              className="grid size-5 shrink-0 place-items-center rounded-chrome text-chrome-muted hover:bg-chrome-menu-active hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
            >
              <ChevronRight
                className={cn(
                  "size-3.5 transition-transform motion-reduce:transition-none",
                  expanded && "rotate-90",
                )}
                aria-hidden="true"
              />
            </button>

            <span
              className="grid size-6 shrink-0 place-items-center rounded-chrome bg-chrome-popup"
              aria-hidden="true"
            >
              <FileIcon className={cn("size-3.5", fileType.tint)} strokeWidth={1.75} />
            </span>

            <button
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
              <span className="mt-0.5 block truncate text-chrome-micro text-chrome-muted">
                {item.savePath}
              </span>
            </button>
          </div>
        </td>

        <td className="border-b border-chrome-line/60 px-3 py-2 text-right tabular-nums text-chrome-menu-text">
          {knownSize ? formatSize(item.size) : "-"}
        </td>

        <td className="border-b border-chrome-line/60 px-3 py-2">
          <div className="flex items-center gap-2">
            <div
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={knownSize ? Math.round(percent) : undefined}
              aria-valuetext={
                knownSize ? `${Math.round(percent)} percent` : "Size unknown"
              }
              aria-label={`${item.fileName} progress`}
              className="h-1.5 flex-1 overflow-hidden rounded-full bg-chrome-line"
            >
              <div
                className={cn("h-full rounded-full", BAR_TINT[item.status])}
                style={{ width: `${knownSize ? percent : 0}%` }}
              />
            </div>
            <span className="w-9 shrink-0 text-right tabular-nums text-chrome-muted">
              {knownSize ? `${Math.round(percent)}%` : "-"}
            </span>
          </div>
        </td>

        <td className="border-b border-chrome-line/60 px-3 py-2 text-right tabular-nums text-chrome-menu-text">
          {formatSpeed(item.speed) || "-"}
        </td>

        <td className="border-b border-chrome-line/60 px-3 py-2 text-right tabular-nums text-chrome-menu-text">
          {item.status === "downloading" ? formatEta(item.eta) : "-"}
        </td>

        <td className="border-b border-chrome-line/60 px-3 py-2">
          <span className="inline-flex items-center gap-1.5 text-chrome-menu-text">
            <StatusIcon
              className={cn("size-3.5 shrink-0", status.tint)}
              aria-hidden="true"
            />
            {STATUS_LABELS[item.status]}
          </span>
        </td>
      </tr>

      {expanded ? (
        <tr id={detailsId} className="bg-chrome-panel">
          <td
            colSpan={columnCount}
            className="border-b border-chrome-line px-3 py-3"
          >
            <dl className="grid grid-cols-1 gap-x-8 gap-y-2 text-xs sm:grid-cols-2 xl:grid-cols-3">
              <Detail label="Address">
                <span className="flex min-w-0 items-start gap-2">
                  <span className="min-w-0 break-all text-chrome-menu-text">
                    {item.url}
                  </span>
                  <CopyButton value={item.url} label="Copy address" />
                </span>
              </Detail>
              <Detail label="Saved to">
                <span className="break-all text-chrome-menu-text">{item.dest}</span>
              </Detail>
              <Detail label="Category">
                <span className="text-chrome-menu-text">{item.category}</span>
              </Detail>
              <Detail label="Received">
                <span className="tabular-nums text-chrome-menu-text">
                  {knownSize
                    ? `${formatSize(item.downloaded)} of ${formatSize(item.size)}`
                    : `${formatSize(item.downloaded)} of unknown size`}
                </span>
              </Detail>
              <Detail label="Connections">
                <span className="tabular-nums text-chrome-menu-text">
                  {item.connections}
                </span>
              </Detail>
              <Detail label="Added">
                <span className="tabular-nums text-chrome-menu-text">
                  {formatDate(item.addedAt)}
                </span>
              </Detail>
            </dl>

            {item.error ? (
              <p
                role="alert"
                className="mt-3 flex items-start gap-2 border-t border-chrome-line/60 pt-2 text-xs text-chrome-error"
              >
                <AlertTriangle
                  className="mt-0.5 size-3.5 shrink-0"
                  aria-hidden="true"
                />
                <span className="min-w-0 break-words">{item.error}</span>
              </p>
            ) : null}
          </td>
        </tr>
      ) : null}
    </>
  );
}

function Detail({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-chrome-micro text-chrome-muted">{label}</dt>
      <dd className="mt-0.5">{children}</dd>
    </div>
  );
}

function CopyButton({ value, label }: { value: string; label: string }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(timer);
  }, [copied]);

  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={() => {
        const clipboard = navigator.clipboard;
        if (!clipboard) {
          return;
        }
        void clipboard
          .writeText(value)
          .then(() => setCopied(true))
          .catch(() => setCopied(false));
      }}
      className="grid size-5 shrink-0 place-items-center rounded-chrome text-chrome-muted hover:bg-chrome-menu-active hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
    >
      {copied ? (
        <Check className="size-3.5 text-icon-green" aria-hidden="true" />
      ) : (
        <Copy className="size-3.5" aria-hidden="true" />
      )}
      <span className="sr-only" aria-live="polite">
        {copied ? "Address copied" : ""}
      </span>
    </button>
  );
}
