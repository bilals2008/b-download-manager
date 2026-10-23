import { useEffect, useMemo, useRef, useState } from "react";

import { FolderGlyph } from "@/components/icons/action-icons";
import { FileTypeIcon, WarningMark } from "@/components/icons/desktop-icons";
import { DownloadGlyph, SpinnerGlyph } from "@/components/icons/ui-icons";
import { Dialog } from "@/components/ui/dialog";
import { ListBox } from "@/components/ui/listbox";
import {
  CATEGORIES,
  categoryOfFile,
  downloadUrl,
  fileNameFromUrl,
  fileNameProblem,
  formatSize,
  joinPath,
  sanitizeFileName,
  splitPath,
  type CategoryName,
} from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import {
  chooseFolder,
  defaultDestination,
  inspectDestination,
  probeLink,
  type DestinationInfo,
} from "@/lib/ipc";
import { loadSettings } from "@/lib/settings";
import { useDownloads } from "@/state/download-store";

interface AddDownloadDialogProps {
  open: boolean;
  onClose: () => void;
}

/** What the server said about the address, in the order the dialog asks. */
type Probe =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "ready"; name: string | null; total: number | null; resumable: boolean }
  | { kind: "unavailable" };

/** The action a click follows: start now, or wait in the queue. */
type Mode = "start" | "later";

const FIELD =
  "w-full rounded-chrome border border-transparent bg-chrome-canvas px-2.5 py-1.5 text-xs text-chrome-menu-text transition-colors placeholder:text-chrome-muted/70 hover:bg-chrome-menu-hover focus-visible:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:opacity-60";

const LABEL = "mb-1 block text-xs text-chrome-muted";

const SECONDARY =
  "rounded-chrome bg-chrome-panel px-3 py-1.5 text-xs text-chrome-menu-text transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:opacity-45";

const PRIMARY =
  "inline-flex items-center gap-1.5 rounded-chrome bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:opacity-50";

/**
 * "Start download" - the dialog IDM opens before it takes a link.
 *
 * The address is the one field that matters, so it sits alone at the top and
 * everything else follows from it: the name the server gives the file, its
 * size and whether the server can resume it arrive from a probe, and the name
 * and category follow the address until the user changes them. The folder is
 * picked with the folder dialog of the system rather than typed.
 *
 * The dialog creates nothing until one of its two start actions succeeds:
 * Cancel, Escape and the close button only close it. A file that is already
 * there is announced once, and the same button is pressed again to replace it.
 */
export function AddDownloadDialog({ open, onClose }: AddDownloadDialogProps) {
  const { addDownload, browserRequest, clearBrowserRequest } = useDownloads();
  const [url, setUrl] = useState("");
  // Null means "follow the address"; a string is the value the user typed.
  const [nameOverride, setNameOverride] = useState<string | null>(null);
  const [categoryOverride, setCategoryOverride] = useState<CategoryName | null>(
    null,
  );
  const [folder, setFolder] = useState("");
  const [description, setDescription] = useState("");
  // The page the browser handed over with the link, if any.
  const [referer, setReferer] = useState("");
  const [probe, setProbe] = useState<Probe>({ kind: "idle" });
  const [destination, setDestination] = useState<DestinationInfo | null>(null);
  const [overwrite, setOverwrite] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<Mode | null>(null);
  // A second click in the same tick reaches the command before React has
  // re-rendered the disabled button; this is the guard that holds.
  const running = useRef(false);

  // Open on a fresh form, whatever the last one left behind.
  useEffect(() => {
    if (!open) {
      return;
    }
    setUrl("");
    setNameOverride(null);
    setCategoryOverride(null);
    setDescription("");
    setReferer("");
    setProbe({ kind: "idle" });
    setDestination(null);
    setOverwrite(false);
    setError(null);
  }, [open]);

  // Start from the folder of the options window, and from the Downloads
  // folder of the system when nothing was chosen there.
  useEffect(() => {
    if (!open) {
      return;
    }
    let active = true;
    const chosen = loadSettings().defaultFolder.trim();
    if (chosen) {
      setFolder(chosen);
      return () => {
        active = false;
      };
    }
    defaultDestination()
      .then((answer) => {
        if (active) {
          setFolder(splitPath(answer).dir || ".");
        }
      })
      .catch(() => {
        // The dialog still works with a typed folder when the engine cannot
        // answer, which is also what happens in a plain browser window.
      });
    return () => {
      active = false;
    };
  }, [open]);

  // A link the browser handed over opens this dialog with the address and the
  // name already in it, and the request is done once they are.
  useEffect(() => {
    if (!open || !browserRequest) {
      return;
    }
    setUrl(browserRequest.url);
    setNameOverride(
      browserRequest.name ? sanitizeFileName(browserRequest.name) : null,
    );
    setCategoryOverride(null);
    setReferer(browserRequest.referer);
    setError(null);
    clearBrowserRequest();
  }, [open, browserRequest, clearBrowserRequest]);

  // Ask the server what the address holds once it has settled: the name it
  // gives the file, the size, and whether a stop could resume. The probe is
  // advice, not a gate: a server that does not answer still gets a download.
  useEffect(() => {
    if (!open) {
      return;
    }
    setProbe({ kind: "idle" });
    if (!isDesktop()) {
      return;
    }
    const address = url.trim();
    if (!downloadUrl(address)) {
      return;
    }
    let active = true;
    setProbe({ kind: "checking" });
    const timer = setTimeout(() => {
      probeLink(address, referer)
        .then((answer) => {
          if (active) {
            setProbe({
              kind: "ready",
              name: answer.name,
              total: answer.total,
              resumable: answer.resumable,
            });
          }
        })
        .catch(() => {
          if (active) {
            setProbe({ kind: "unavailable" });
          }
        });
    }, 500);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [open, url, referer]);

  // The name and the category are derived, never copied into state, so
  // editing the address keeps them in step until the user overrides one.
  const detectedName = useMemo(
    () => (url.trim() ? sanitizeFileName(fileNameFromUrl(url.trim())) : ""),
    [url],
  );
  // The server's name wins: a link like `/get?id=12` carries no useful name,
  // and one like `/download.php` carries a name the server overrides.
  const serverName =
    probe.kind === "ready" && probe.name
      ? sanitizeFileName(probe.name)
      : null;
  const name = nameOverride ?? serverName ?? detectedName;
  const category =
    categoryOverride ??
    (name ? categoryOfFile(name) : "General");
  const target = folder.trim() && name.trim()
    ? joinPath(folder.trim(), name.trim())
    : "";

  // Announce an existing file before the second click replaces it. The check
  // is a debounce, so typing a name does not ask the engine for every letter.
  useEffect(() => {
    setDestination(null);
    setOverwrite(false);
    if (!open || !isDesktop() || !target) {
      return;
    }
    let active = true;
    const timer = setTimeout(() => {
      inspectDestination(target)
        .then((answer) => {
          if (active) {
            setDestination(answer);
          }
        })
        .catch(() => {
          // A half-typed folder is not an error yet; Start checks again and
          // shows what the engine says.
        });
    }, 400);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [open, target]);

  // A change to the name or the folder is a new destination, so a question
  // about replacing a file no longer applies to what is on screen.
  function forgetOverwrite() {
    setOverwrite(false);
    setError(null);
  }

  const sizeLabel =
    probe.kind === "ready"
      ? probe.total === null
        ? "Unknown size"
        : formatSize(probe.total)
      : null;
  const resumeLabel =
    probe.kind === "ready"
      ? probe.resumable
        ? "Resumable"
        : "Not resumable"
      : null;
  const probeLabel =
    probe.kind === "checking"
      ? "Checking size and resume support…"
      : probe.kind === "unavailable"
        ? "Size and resume support not available"
        : sizeLabel && resumeLabel
          ? `${sizeLabel} · ${resumeLabel}`
          : null;
  const willReplace = overwrite && destination?.fileExists === true;

  function reset() {
    setUrl("");
    setNameOverride(null);
    setCategoryOverride(null);
    setDescription("");
    setReferer("");
    setProbe({ kind: "idle" });
    setDestination(null);
    setOverwrite(false);
    setError(null);
  }

  function close() {
    if (running.current) {
      return;
    }
    reset();
    onClose();
  }

  async function pickFolder() {
    if (!isDesktop()) {
      setError("The folder picker is only available in the desktop window.");
      return;
    }
    try {
      const picked = await chooseFolder(folder.trim());
      if (picked) {
        setFolder(picked);
        forgetOverwrite();
      }
    } catch (failure) {
      setError(describeError(failure));
    }
  }

  async function submit(mode: Mode) {
    if (running.current) {
      return;
    }
    setError(null);

    const address = url.trim();
    if (!downloadUrl(address)) {
      setError(
        "Enter the full address of the file, starting with http://, https:// or ftp://.",
      );
      return;
    }
    const chosenName = name.trim();
    const nameIssue = fileNameProblem(chosenName);
    if (nameIssue) {
      setError(nameIssue);
      return;
    }
    const chosenFolder = folder.trim();
    if (!chosenFolder) {
      setError("Choose the folder the file should be saved to.");
      return;
    }
    if (!isDesktop()) {
      setError("The download engine is only available in the desktop window.");
      return;
    }

    const dest = joinPath(chosenFolder, chosenName);
    running.current = true;
    setBusy(mode);
    try {
      const check = await inspectDestination(dest);
      setDestination(check);
      if (check.fileExists && !overwrite) {
        // The same button starts it the second time. Nothing is created yet.
        setOverwrite(true);
        return;
      }
      await addDownload({
        url: address,
        fileName: chosenName,
        category,
        savePath: chosenFolder,
        referer,
        queue: mode === "later",
        description,
      });
      reset();
      onClose();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      running.current = false;
      setBusy(null);
    }
  }

  return (
    <Dialog
      open={open}
      title="Start download"
      onClose={close}
      className="w-[min(34rem,calc(100vw-2rem))]"
      footerClassName="items-center gap-2"
      footer={
        <>
          <button
            type="button"
            onClick={close}
            disabled={busy !== null}
            className={`mr-auto ${SECONDARY}`}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void submit("later")}
            disabled={busy !== null}
            className={`inline-flex items-center gap-1.5 ${SECONDARY}`}
          >
            {busy === "later" ? (
              <SpinnerGlyph className="size-3.5 motion-safe:animate-spin" />
            ) : null}
            {busy === "later" ? "Adding…" : "Download later"}
          </button>
          <button
            type="submit"
            form="add-download-form"
            disabled={busy !== null}
            className={PRIMARY}
          >
            {busy === "start" ? (
              <SpinnerGlyph className="size-3.5 motion-safe:animate-spin" />
            ) : (
              <DownloadGlyph className="size-3.5" />
            )}
            {busy === "start"
              ? "Starting…"
              : willReplace
                ? "Replace file"
                : "Start download"}
          </button>
        </>
      }
    >
      <form
        id="add-download-form"
        onSubmit={(event) => {
          event.preventDefault();
          void submit("start");
        }}
        aria-busy={busy !== null}
        className="flex flex-col gap-3.5"
      >
        <div>
          <label htmlFor="add-url" className={LABEL}>
            Address of the file
          </label>
          <input
            id="add-url"
            className={`${FIELD} font-mono`}
            value={url}
            autoFocus
            spellCheck={false}
            onChange={(event) => {
              setUrl(event.target.value);
              setError(null);
              setOverwrite(false);
            }}
            placeholder="https://example.org/file.zip"
          />
        </div>

        {/* What the address adds up to, before the user changes any of it:
            the file it becomes, its size and whether it can be resumed. It is
            the answer to "did I paste the right link". */}
        {url.trim() ? (
          <div className="flex items-center gap-3" aria-live="polite">
            <FileTypeIcon category={category} className="size-7 shrink-0" />
            <div className="min-w-0 flex-1">
              <p className="truncate text-xs text-chrome-title-text">
                {name || "Taking the name from the address…"}
              </p>
              <p className="truncate text-chrome-micro text-chrome-muted">
                {[category, probeLabel].filter(Boolean).join(" · ")}
              </p>
            </div>
          </div>
        ) : null}

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-[minmax(0,1fr)_9rem]">
          <div>
            <label htmlFor="add-name" className={LABEL}>
              File name
            </label>
            <input
              id="add-name"
              className={FIELD}
              value={name}
              spellCheck={false}
              onChange={(event) => {
                setNameOverride(event.target.value);
                forgetOverwrite();
              }}
              placeholder="Taken from the address"
            />
          </div>
          <div>
            <span className={LABEL}>Category</span>
            <ListBox
              label="Category"
              value={category}
              onChange={(value) => {
                setCategoryOverride(value as CategoryName);
                setError(null);
              }}
              disabled={busy !== null}
              options={CATEGORIES.map((value) => ({ value, label: value }))}
            />
          </div>
        </div>

        <div>
          <label htmlFor="add-folder" className={LABEL}>
            Save to folder
          </label>
          <div className="flex gap-2">
            <input
              id="add-folder"
              className={FIELD}
              value={folder}
              spellCheck={false}
              onChange={(event) => {
                setFolder(event.target.value);
                forgetOverwrite();
              }}
            />
            <button
              type="button"
              onClick={() => void pickFolder()}
              disabled={busy !== null}
              className={`inline-flex shrink-0 items-center gap-1.5 ${SECONDARY}`}
            >
              <FolderGlyph className="size-3.5" />
              Browse…
            </button>
          </div>
          <p
            title={target || undefined}
            className="mt-1.5 truncate font-mono text-chrome-micro text-chrome-muted"
          >
            {target ? `Saves as ${target}` : "The file name is taken from the address when the box is left empty."}
          </p>
          {destination && !destination.folderExists ? (
            <p className="mt-1 text-chrome-micro text-chrome-muted">
              This folder does not exist yet; it will be created.
            </p>
          ) : null}
        </div>

        <div>
          <label htmlFor="add-description" className={LABEL}>
            Description <span className="text-chrome-muted">(optional)</span>
          </label>
          <input
            id="add-description"
            className={FIELD}
            value={description}
            maxLength={500}
            spellCheck={false}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="A note for this download"
          />
        </div>

        {willReplace ? (
          <p className="flex items-start gap-2 text-xs text-chrome-menu-text">
            <WarningMark tone="amber" className="mt-0.5 size-3.5 shrink-0" />
            <span className="min-w-0 break-words">
              A file named <span className="font-mono">{name.trim()}</span> is
              already in that folder. Press the button again to replace it.
            </span>
          </p>
        ) : null}

        {error ? (
          <p role="alert" className="flex items-start gap-2 text-xs text-chrome-error">
            <WarningMark tone="red" className="mt-0.5 size-3.5 shrink-0" />
            <span className="min-w-0 break-words">{error}</span>
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
