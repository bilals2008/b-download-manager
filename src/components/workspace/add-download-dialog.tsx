import { useState } from "react";

import { Dialog } from "@/components/ui/dialog";
import { ListBox } from "@/components/ui/listbox";
import { CATEGORIES, type CategoryName } from "@/lib/downloads";
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

  function reset() {
    setUrl("");
    setFileName("");
    setCategory("General");
    setSavePath("C:\\Downloads");
    setError(null);
  }

  function submit(event: React.FormEvent) {
    event.preventDefault();
    const trimmed = url.trim();
    if (!/^https?:\/\/\S+$/i.test(trimmed)) {
      setError("Enter the full address of the file, starting with http:// or https://.");
      return;
    }
    const name = fileName.trim() || trimmed.split("/").pop() || "download";
    addDownload({ url: trimmed, fileName: name, category, savePath: savePath.trim() });
    reset();
    onClose();
  }

  return (
    <Dialog
      open={open}
      title="Add new download"
      onClose={() => {
        reset();
        onClose();
      }}
      footer={
        <>
          <button
            type="button"
            onClick={() => {
              reset();
              onClose();
            }}
            className="rounded-chrome border border-chrome-line px-3 py-1 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Cancel
          </button>
          <button
            type="submit"
            form="add-download-form"
            className="rounded-chrome bg-chrome-menu-active px-3 py-1 text-xs text-chrome-title-text hover:bg-chrome-popup-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            OK
          </button>
        </>
      }
    >
      <form id="add-download-form" onSubmit={submit} className="flex flex-col gap-3">
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
        <div className="grid grid-cols-2 gap-3">
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
              Save to
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
