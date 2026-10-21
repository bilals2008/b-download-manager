import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";

import { Dialog } from "@/components/ui/dialog";
import { ListBox } from "@/components/ui/listbox";
import {
  CATEGORIES,
  fileNameFromUrl,
  sanitizeFileName,
  splitPath,
  type CategoryName,
} from "@/lib/downloads";
import { describeError } from "@/lib/errors";
import { defaultDestination } from "@/lib/ipc";
import { useDownloads } from "@/state/download-store";

interface AddDownloadDialogProps {
  open: boolean;
  onClose: () => void;
}

const FIELD =
  "w-full rounded-chrome border border-chrome-line bg-chrome-canvas px-2 py-1.5 text-xs text-chrome-menu-text placeholder:text-chrome-muted/70 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none";

const LABEL = "mb-1 block text-xs text-chrome-muted";

/**
 * "Add new download" - the dialog IDM opens from Tasks and from the toolbar.
 * The category picker is a listbox rather than a native select, so it follows
 * the theme and keeps full keyboard support.
 */
export function AddDownloadDialog({ open, onClose }: AddDownloadDialogProps) {
  const { addDownload } = useDownloads();
  const [url, setUrl] = useState("");
  const [fileName, setFileName] = useState("");
  const [category, setCategory] = useState<CategoryName>("General");
  const [savePath, setSavePath] = useState("C:\\Downloads");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Start from the real Downloads folder instead of a guess.
  useEffect(() => {
    if (!open) {
      return;
    }
    let active = true;
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

    const name = sanitizeFileName(fileName.trim() || fileNameFromUrl(trimmed));
    setBusy(true);
    setError(null);
    try {
      await addDownload({ url: trimmed, fileName: name, category, savePath: folder });
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
            className="inline-flex items-center gap-1.5 rounded-chrome bg-chrome-menu-active px-3 py-1 text-xs text-chrome-title-text hover:bg-chrome-popup-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:opacity-60"
          >
            {busy ? (
              <LoaderCircle
                className="size-3.5 motion-safe:animate-spin"
                aria-hidden="true"
              />
            ) : null}
            {busy ? "Starting…" : "OK"}
          </button>
        </>
      }
    >
      <form
        id="add-download-form"
        onSubmit={submit}
        aria-busy={busy}
        className="flex flex-col gap-3"
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
            onChange={(event) => {
              setUrl(event.target.value);
              setError(null);
            }}
            placeholder="https://example.org/file.zip"
          />
        </div>
        <div>
          <label htmlFor="add-name" className={LABEL}>
            File name
          </label>
          <input
            id="add-name"
            className={FIELD}
            value={fileName}
            onChange={(event) => setFileName(event.target.value)}
            placeholder="Taken from the address when left empty"
          />
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div>
            <span className={LABEL}>Category</span>
            <ListBox
              label="Category"
              value={category}
              onChange={(value) => setCategory(value as CategoryName)}
              options={CATEGORIES.map((name) => ({ value: name, label: name }))}
            />
          </div>
          <div>
            <label htmlFor="add-path" className={LABEL}>
              Save to folder
            </label>
            <input
              id="add-path"
              className={FIELD}
              value={savePath}
              onChange={(event) => setSavePath(event.target.value)}
            />
          </div>
        </div>
        {error ? (
          <p role="alert" className="text-xs text-chrome-error">
            {error}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
