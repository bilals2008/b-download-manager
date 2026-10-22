import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Download, LoaderCircle } from "lucide-react";

import { Dialog } from "@/components/ui/dialog";
import { ListBox } from "@/components/ui/listbox";
import {
  CATEGORIES,
  fileNameFromUrl,
  joinPath,
  sanitizeFileName,
  splitPath,
  type CategoryName,
} from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { defaultDestination } from "@/lib/ipc";
import { loadSettings } from "@/lib/settings";
import { useDownloads } from "@/state/download-store";

interface AddDownloadDialogProps {
  open: boolean;
  onClose: () => void;
}

const FIELD =
  "w-full rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 py-1.5 text-xs text-chrome-menu-text placeholder:text-chrome-muted/70 hover:border-chrome-muted/60 focus-visible:border-chrome-focus focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none";

const LABEL = "mb-1 block text-xs text-chrome-muted";

/**
 * "Add new download" - the dialog IDM opens from Tasks and from the toolbar.
 *
 * The address is the one field that matters, so it sits alone at the top and
 * everything else follows from it: the file name is taken from the address
 * when the box is left empty, and the line at the bottom shows where the
 * file will land. The category picker is a listbox rather than a native
 * select, so it follows the theme and keeps full keyboard support.
 */
export function AddDownloadDialog({ open, onClose }: AddDownloadDialogProps) {
  const { addDownload } = useDownloads();
  const [url, setUrl] = useState("");
  const [fileName, setFileName] = useState("");
  const [category, setCategory] = useState<CategoryName>("General");
  const [savePath, setSavePath] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Start from the folder of the options window, and from the Downloads
  // folder of the system when nothing was chosen there.
  useEffect(() => {
    if (!open) {
      return;
    }
    let active = true;
    const chosen = loadSettings().defaultFolder.trim();
    if (chosen) {
      setSavePath(chosen);
      return () => {
        active = false;
      };
    }
    defaultDestination()
      .then((destination) => {
        if (!active) {
          return;
        }
        const folder = splitPath(destination).dir;
        setSavePath(folder || ".");
      })
      .catch(() => {
        // The dialog still works with the typed folder when the engine cannot
        // answer, which is also what happens in a plain browser window.
      });
    return () => {
      active = false;
    };
  }, [open]);

  function reset() {
    setUrl("");
    setFileName("");
    setCategory("General");
    setError(null);
  }

  function close() {
    reset();
    onClose();
  }

  const name = useMemo(
    () => sanitizeFileName(fileName.trim() || fileNameFromUrl(url.trim())),
    [fileName, url],
  );

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (busy) {
      return;
    }
    const trimmed = url.trim();
    if (!/^(https?|ftp):\/\/\S+$/i.test(trimmed)) {
      setError(
        "Enter the full address of the file, starting with http://, https:// or ftp://.",
      );
      return;
    }
    const folder = savePath.trim();
    if (!folder) {
      setError("Choose the folder the file should be saved to.");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      await addDownload({
        url: trimmed,
        fileName: name,
        category,
        savePath: folder,
      });
      reset();
      onClose();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      open={open}
      title="Add new download"
      onClose={close}
      className="w-[min(36rem,calc(100vw-2rem))]"
      footer={
        <>
          <button
            type="button"
            onClick={close}
            className="rounded-chrome border border-chrome-line px-3 py-1 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Cancel
          </button>
          <button
            type="submit"
            form="add-download-form"
            disabled={busy}
            className="inline-flex items-center gap-1.5 rounded-chrome bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:opacity-90 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:opacity-60"
          >
            {busy ? (
              <LoaderCircle
                className="size-3.5 motion-safe:animate-spin"
                aria-hidden="true"
              />
            ) : (
              <Download className="size-3.5" aria-hidden="true" />
            )}
            {busy ? "Starting…" : "Start download"}
          </button>
        </>
      }
    >
      <form
        id="add-download-form"
        onSubmit={submit}
        aria-busy={busy}
        className="flex flex-col gap-4"
      >
        <div>
          <label htmlFor="add-url" className={LABEL}>
            Address of the file
          </label>
          <input
            id="add-url"
            className={FIELD}
            value={url}
            autoFocus
            spellCheck={false}
            onChange={(event) => {
              setUrl(event.target.value);
              setError(null);
            }}
            placeholder="https://example.org/file.zip"
          />
        </div>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <div>
            <label htmlFor="add-name" className={LABEL}>
              File name
            </label>
            <input
              id="add-name"
              className={FIELD}
              value={fileName}
              spellCheck={false}
              onChange={(event) => setFileName(event.target.value)}
              placeholder={fileNameFromUrl(url.trim()) || "Taken from the address"}
            />
          </div>
          <div>
            <span className={LABEL}>Category</span>
            <ListBox
              label="Category"
              value={category}
              onChange={(value) => setCategory(value as CategoryName)}
              options={CATEGORIES.map((name) => ({ value: name, label: name }))}
            />
          </div>
        </div>

        <div>
          <label htmlFor="add-path" className={LABEL}>
            Save to folder
          </label>
          <input
            id="add-path"
            className={FIELD}
            value={savePath}
            spellCheck={false}
            onChange={(event) => setSavePath(event.target.value)}
          />
          <p className="mt-1.5 truncate text-chrome-micro text-chrome-muted">
            {name && savePath.trim()
              ? `Saves as ${joinPath(savePath.trim(), name)}`
              : "The file name is taken from the address when the box is left empty."}
          </p>
        </div>

        {error ? (
          <p
            role="alert"
            className="flex items-start gap-2 text-xs text-chrome-error"
          >
            <AlertTriangle className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
            <span className="min-w-0 break-words">{error}</span>
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
