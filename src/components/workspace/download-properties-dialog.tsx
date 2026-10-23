import { CopyGlyph } from "@/components/icons/action-icons";
import { Dialog } from "@/components/ui/dialog";
import { copyText } from "@/lib/clipboard";
import {
  formatDate,
  formatSize,
  progressOf,
  STATUS_LABELS,
  type Download,
} from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

interface DownloadPropertiesDialogProps {
  /** The download to describe. Null keeps the dialog closed. */
  item: Download | null;
  onClose: () => void;
}

/**
 * The properties of one download, the way IDM shows them.
 *
 * Read-only on purpose: everything here is a fact the engine reported, and a
 * field a user could edit would have to be sent back to the engine first.
 */
export function DownloadPropertiesDialog({
  item,
  onClose,
}: DownloadPropertiesDialogProps) {
  const { reportError } = useDownloads();

  async function copyAddress() {
    if (item && !(await copyText(item.url))) {
      reportError("The address could not be copied to the clipboard.");
    }
  }

  const percent = item ? progressOf(item) : 0;

  return (
    <Dialog
      open={item !== null}
      title={item ? `${item.fileName} Properties` : "Properties"}
      onClose={onClose}
      className="w-[min(34rem,calc(100vw-2rem))]"
      footer={
        <>
          <button
            type="button"
            onClick={copyAddress}
            className="mr-auto inline-flex items-center gap-1.5 rounded-chrome bg-chrome-panel px-3 py-1.5 text-xs text-chrome-menu-text transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <CopyGlyph className="size-3.5" />
            Copy address
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-chrome bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Close
          </button>
        </>
      }
    >
      {item ? (
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-start gap-x-4 gap-y-1.5 text-xs">
          <Field label="File name">{item.fileName}</Field>
          <Field label="Address">
            <span className="font-mono break-all">{item.url}</span>
          </Field>
          <Field label="Save folder">
            <span className="font-mono break-all">{item.savePath}</span>
          </Field>
          <Field label="Size">
            {item.size > 0
              ? `${formatSize(item.size)} (${item.size.toLocaleString()} bytes)`
              : "Not known yet"}
          </Field>
          <Field label="Downloaded">
            {item.size > 0
              ? `${formatSize(item.downloaded)} of ${formatSize(item.size)} (${percent.toFixed(2)}%)`
              : formatSize(item.downloaded)}
          </Field>
          <Field label="Status">{STATUS_LABELS[item.status]}</Field>
          <Field label="Category">{item.category}</Field>
          {item.description ? (
            <Field label="Description">
              <span className="break-words">{item.description}</span>
            </Field>
          ) : null}
          <Field label="Connections">{item.connections}</Field>
          <Field label="Added">{formatDate(item.addedAt)}</Field>
          <Field label="Last try">{formatDate(item.lastTryAt)}</Field>
          {item.error ? (
            <Field label="Error">
              <span className="text-chrome-error">{item.error}</span>
            </Field>
          ) : null}
        </dl>
      ) : null}
    </Dialog>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <>
      <dt className="text-chrome-muted">{label}</dt>
      <dd className="min-w-0 select-text text-chrome-menu-text">{children}</dd>
    </>
  );
}
