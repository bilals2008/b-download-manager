import { useEffect, useMemo, useState } from "react";

import { WarningMark } from "@/components/icons/desktop-icons";
import {
  DownloadGlyph,
  SpinnerGlyph,
} from "@/components/icons/ui-icons";
import { Dialog } from "@/components/ui/dialog";
import { ListBox } from "@/components/ui/listbox";
import {
  CATEGORIES,
  categoryOfFile,
  fileNameFromUrl,
  joinPath,
  sanitizeFileName,
  splitPath,
  type CategoryName,
} from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import { chooseFolder, defaultDestination, probeFileName } from "@/lib/ipc";
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
 * everything else follows from it: the file name and the category come from
 * the address until the user changes them, and the folder is picked with the
 * folder dialog of the system rather than typed. The category picker is a
 * listbox rather than a native select, so it follows the theme and keeps full
 * keyboard support.
 */
export function AddDownloadDialog({ open, onClose }: AddDownloadDialogProps) {
  const { addDownload } = useDownloads();
  const [url, setUrl] = useState("");
  // Null means "follow the address"; a string is the value the user typed or
  // picked, which the address then stops overriding.
  const [nameOverride, setNameOverride] = useState<string | null>(null);
  const [categoryOverride, setCategoryOverride] = useState<CategoryName | null>(
    null,
  );
  const [savePath, setSavePath] = useState("");
  // The name the server gives a link that carries none of its own.
  const [serverName, setServerName] = useState<string | null>(null);
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

  // A link like `https://example.org/get?id=12` carries no file name, but the
  // answer of the server does, in its `Content-Disposition` header. Ask once
  // the address has settled, and only when the address gave no name itself.
  useEffect(() => {
    setServerName(null);
    if (!open || !isDesktop()) {
      return;
    }
    const trimmed = url.trim();
    if (!/^(https?|ftp):\/\/\S+$/i.test(trimmed)) {
      return;
    }
    if (/\.[a-z0-9]{1,8}$/i.test(fileNameFromUrl(trimmed))) {
      return;
    }
    let active = true;
    const timer = setTimeout(() => {
      probeFileName(trimmed)
        .then((found) => {
          if (active && found) {
            setServerName(sanitizeFileName(found));
          }
        })
        .catch(() => {
          // No answer from the server: the address stays the only source.
        });
    }, 600);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [open, url]);

  function reset() {
    setUrl("");
    setNameOverride(null);
    setCategoryOverride(null);
    setError(null);
  }

  function close() {
    reset();
    onClose();
  }

  // The name and the category are derived, never copied into state, so
  // editing the address keeps them in step until the user overrides one.
  const detectedName = useMemo(
    () => (url.trim() ? sanitizeFileName(fileNameFromUrl(url.trim())) : ""),
    [url],
  );
  const name = sanitizeFileName(
    nameOverride ??
      (serverName || detectedName || fileNameFromUrl(url.trim())),
  );
  const category =
    categoryOverride ?? (url.trim() ? categoryOfFile(name) : "General");

  async function pickFolder() {
    if (!isDesktop()) {
      setError("The folder picker is only available in the desktop window.");
      return;
    }
    try {
      const picked = await chooseFolder(savePath);
      if (picked) {
        setSavePath(picked);
        setError(null);
      }
    } catch (failure) {
      setError(describeError(failure));
    }
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
              <SpinnerGlyph className="size-3.5 motion-safe:animate-spin" />
            ) : (
              <DownloadGlyph className="size-3.5" />
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
              value={nameOverride ?? (serverName || detectedName)}
              spellCheck={false}
              onChange={(event) => setNameOverride(event.target.value)}
              placeholder="Taken from the address"
            />
          </div>
          <div>
            <span className={LABEL}>Category</span>
            <ListBox
              label="Category"
              value={category}
              onChange={(value) => setCategoryOverride(value as CategoryName)}
              options={CATEGORIES.map((name) => ({ value: name, label: name }))}
            />
          </div>
        </div>

        <div>
          <label htmlFor="add-path" className={LABEL}>
            Save to folder
          </label>
          <div className="flex gap-2">
            <input
              id="add-path"
              className={FIELD}
              value={savePath}
              spellCheck={false}
              onChange={(event) => setSavePath(event.target.value)}
            />
            <button
              type="button"
              onClick={pickFolder}
              className="shrink-0 rounded-chrome border border-chrome-line px-3 py-1 text-xs text-chrome-menu-text hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
            >
              Browse…
            </button>
          </div>
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
            <WarningMark tone="red" className="mt-0.5 size-3.5 shrink-0" />
            <span className="min-w-0 break-words">{error}</span>
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
