import { useState } from "react";

import { FolderGlyph, RefreshGlyph } from "@/components/icons/action-icons";
import { FileTypeIcon, WarningMark } from "@/components/icons/desktop-icons";
import { ChevronDown, ChevronUp } from "@/components/icons/ui-icons";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { Tooltip } from "@/components/ui/tooltip";
import {
  formatSize,
  STATUS_LABELS,
  type Download,
} from "@/lib/downloads";
import {
  explainFailure,
  receivedSummary,
  resumeSummary,
} from "@/lib/failures";
import { cn } from "@/lib/utils";
import { useDownloads } from "@/state/download-store";

interface DownloadFailedDialogProps {
  item: Download;
  /** A recovery command is in flight; every control waits for it. */
  busy: boolean;
  /** What the engine said about the last recovery command. */
  actionError: string | null;
  onClose: () => void;
  /** `restart` drops the bytes of the failed attempt and starts over. */
  onRetry: (restart: boolean) => void;
  /** Pick another folder, parts included, and continue from them. */
  onChooseFolder: () => void;
}

/**
 * The Download Failed window: why it stopped, where the file was going, and
 * the one recovery action that fits.
 *
 * The headline is the failure in a sentence, the hint is what to do about it,
 * and the engine's own message waits behind Technical details instead of
 * filling the window. Retry continues from the bytes on disk when the engine
 * can, Start over forgets them, and a file that cannot be written where it was
 * given can be moved without losing what already arrived.
 */
export function DownloadFailedDialog({
  item,
  busy,
  actionError,
  onClose,
  onRetry,
  onChooseFolder,
}: DownloadFailedDialogProps) {
  const { categories } = useDownloads();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const explanation = explainFailure(item.error);
  const canRetry = explanation.retryable;
  const resumable = item.resumable === true;
  // A file the server promised in another size cannot be fixed by joining
  // the same ranges again, so the retry starts over instead of resuming.
  const restartFirst = explanation.kind === "size";
  const retryLabel = restartFirst ? "Start over" : resumable ? "Resume" : "Retry";
  const knownSize = item.size > 0;

  return (
    <Dialog
      open
      title="Download failed"
      onClose={onClose}
      dismissible={!busy}
      className="w-[min(28rem,calc(100vw-2rem))]"
      footerClassName="flex-wrap"
      footer={
        <>
          <DialogAction tone="quiet" onClick={onClose} disabled={busy}>
            Close
          </DialogAction>

          {canRetry ? (
            <DialogAction
              icon={<FolderGlyph className="size-3.5" />}
              onClick={onChooseFolder}
              disabled={busy}
            >
              Choose another location
            </DialogAction>
          ) : null}

          {canRetry && resumable && !restartFirst ? (
            <DialogAction onClick={() => onRetry(true)} disabled={busy}>
              Start over
            </DialogAction>
          ) : null}

          {canRetry ? (
            <DialogAction
              tone="primary"
              data-autofocus
              icon={<RefreshGlyph className="size-3.5" />}
              onClick={() => onRetry(restartFirst)}
              disabled={busy}
            >
              {retryLabel}
            </DialogAction>
          ) : (
            <DialogAction
              tone="primary"
              data-autofocus
              icon={<FolderGlyph className="size-3.5" />}
              onClick={onChooseFolder}
              disabled={busy}
            >
              Choose another location
            </DialogAction>
          )}
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-start gap-2">
          <WarningMark
            tone="red"
            className="mt-px size-3.5 shrink-0"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-xs text-chrome-title-text">{explanation.headline}</p>
            <p className="mt-0.5 text-chrome-micro text-chrome-muted">
              {explanation.hint}
            </p>
          </div>
        </div>

        <div className="flex min-w-0 items-start gap-2.5">
          <FileTypeIcon
            category={item.category}
            icon={
              categories.find((entry) => entry.name === item.category)?.icon
            }
            className="mt-px size-6 shrink-0"
          />
          <div className="min-w-0 flex-1">
            <Tooltip content={item.fileName} side="top" className="block min-w-0">
              <p className="truncate text-xs text-chrome-menu-text">
                {item.fileName}
              </p>
            </Tooltip>
            <Tooltip content={item.dest} side="top" className="block min-w-0">
              <p className="truncate font-mono text-chrome-micro text-chrome-muted">
                {item.dest}
              </p>
            </Tooltip>
          </div>
        </div>

        <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-3 gap-y-1 text-chrome-micro">
          <dt className="text-chrome-muted">State</dt>
          <dd className="min-w-0 truncate text-chrome-menu-text">
            {STATUS_LABELS[item.status]}
            {" · "}
            {receivedSummary(item)}
            {knownSize && item.downloaded > 0 ? (
              <span className="text-chrome-muted">
                {" "}
                ({formatSize(item.downloaded)} of {formatSize(item.size)})
              </span>
            ) : null}
          </dd>
          <dt className="text-chrome-muted">Continue</dt>
          <dd className="min-w-0 truncate text-chrome-menu-text">
            {resumeSummary(item.resumable)}
          </dd>
        </dl>

        {actionError ? (
          <p role="alert" className="text-chrome-micro text-chrome-error">
            {actionError}
          </p>
        ) : null}

        <div>
          <button
            type="button"
            aria-expanded={detailsOpen}
            onClick={() => setDetailsOpen((open) => !open)}
            className="inline-flex items-center gap-1 rounded-chrome text-chrome-micro text-chrome-muted transition-colors hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            {detailsOpen ? (
              <ChevronUp className="size-3.5" />
            ) : (
              <ChevronDown className="size-3.5" />
            )}
            Technical details
          </button>
          <pre
            className={cn(
              "mt-1.5 max-h-24 overflow-auto rounded-chrome bg-chrome-canvas p-2 font-mono text-chrome-micro break-words whitespace-pre-wrap text-chrome-menu-text",
              !detailsOpen && "hidden",
            )}
          >
            {item.error?.trim() || "No reason was reported."}
          </pre>
        </div>
      </div>
    </Dialog>
  );
}
