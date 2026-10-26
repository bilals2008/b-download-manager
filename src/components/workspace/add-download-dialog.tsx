import { useEffect, useMemo, useRef, useState } from "react";

import { FolderGlyph } from "@/components/icons/action-icons";
import { FileTypeIcon, WarningMark } from "@/components/icons/desktop-icons";
import { ChevronDown, DownloadGlyph, SpinnerGlyph } from "@/components/icons/ui-icons";
import { MenuPopup, type MenuRow } from "@/components/shell/menu-popup";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { Tooltip } from "@/components/ui/tooltip";
import {
  categoryFolder,
  detectCategory,
  downloadUrl,
  fileNameFromUrl,
  formatSize,
  joinPath,
  probeWarning,
  safeFileName,
  sameAddress,
  splitPath,
  urlProblem,
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
import { preferredFolder, saveLastFolder } from "@/lib/settings";
import { useDownloads } from "@/state/download-store";

interface AddDownloadDialogProps {
  open: boolean;
  onClose: () => void;
  /** An address to open with, from a link watched on the clipboard. */
  seedUrl?: string | null;
}

/** What the server said about the address, in the order the dialog asks. */
type Probe =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "ready"; name: string | null; total: number | null; resumable: boolean }
  | { kind: "failed"; reason: string };

/** A link the browser extension handed over, while it is still this address. */
interface HandedOver {
  url: string;
  name: string;
  referer: string;
}

const LABEL = "mb-1.5 block text-xs text-chrome-muted";

const FOLDER_ROW =
  "group flex h-9 w-full min-w-0 items-center gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 text-left transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-60 disabled:hover:border-chrome-line";

/**
 * "Add URL" - the compact way to take a link without a browser.
 *
 * The address is the only field that matters, so it sits alone at the top and
 * everything else follows from it: the name the server gives the file, its
 * size, whether it can be resumed and the category it lands in all arrive from
 * a probe, and the folder below is the one the file goes into. The link is
 * checked as it settles, but the check is advice rather than a gate: a server
 * that will not answer the probe can still be downloaded from.
 *
 * The dialog creates nothing until Download succeeds: Cancel, Escape and the
 * close button only close it, and a start in flight cannot be dismissed into a
 * download that appears from nowhere. A file that is already there is the
 * engine's question to ask, so the File Exists window answers it after the
 * start, not a second press of the same button.
 */
export function AddDownloadDialog({
  open,
  onClose,
  seedUrl,
}: AddDownloadDialogProps) {
  const {
    addDownload,
    browserRequest,
    categories,
    clearBrowserRequest,
    downloads: allDownloads,
  } = useDownloads();
  const [url, setUrl] = useState("");
  const [handedOver, setHandedOver] = useState<HandedOver | null>(null);
  const [folder, setFolder] = useState("");
  // Null follows the file name; a pick holds the category the user chose.
  const [categoryOverride, setCategoryOverride] = useState<CategoryName | null>(
    null,
  );
  const [categoryAt, setCategoryAt] = useState<{ x: number; y: number } | null>(
    null,
  );
  const [probe, setProbe] = useState<Probe>({ kind: "idle" });
  const [destination, setDestination] = useState<DestinationInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  // A second click in the same tick reaches the command before React has
  // re-rendered the disabled button; this is the guard that holds.
  const running = useRef(false);
  // The address is the one thing the user came for, so it has the caret
  // whenever the dialog opens, including after a browser hand-off. The
  // attribute carries that to `Dialog`, which focuses it after `showModal`.
  const urlField = useRef<HTMLInputElement>(null);
  const folderButton = useRef<HTMLButtonElement>(null);
  const categoryButton = useRef<HTMLButtonElement>(null);
  // A folder the user picked is theirs until the dialog closes, so a category
  // folder never overwrites it. A category folder applied below does the same
  // for the slower default the engine answers with.
  const folderPicked = useRef(false);
  const categoryFolderApplied = useRef(false);
  const baseFolder = useRef("");

  // Open on a fresh form, whatever the last one left behind. A link the
  // clipboard watch offered is the only thing it carries in.
  useEffect(() => {
    if (!open) {
      return;
    }
    setUrl(seedUrl?.trim() ?? "");
    setHandedOver(null);
    setFolder("");
    folderPicked.current = false;
    categoryFolderApplied.current = false;
    baseFolder.current = "";
    setCategoryOverride(null);
    setCategoryAt(null);
    setProbe({ kind: "idle" });
    setDestination(null);
    setError(null);
    setTouched(false);
    setBusy(false);
  }, [open, seedUrl]);

  // Start from the folder of the last download, then from the folder of the
  // options window, and from the Downloads folder of the system when neither
  // has one.
  useEffect(() => {
    if (!open) {
      return;
    }
    let active = true;
    const remembered = preferredFolder();
    if (remembered) {
      baseFolder.current = remembered;
      setFolder(remembered);
      return () => {
        active = false;
      };
    }
    defaultDestination()
      .then((answer) => {
        if (active && !folderPicked.current && !categoryFolderApplied.current) {
          const target = splitPath(answer).dir || ".";
          baseFolder.current = target;
          setFolder(target);
        }
      })
      .catch(() => {
        // The dialog still works after the folder is picked when the engine
        // cannot answer, which is also what happens in a plain browser window.
      });
    return () => {
      active = false;
    };
  }, [open]);

  // A link the browser handed over opens this dialog with its address, its
  // name and the page it came from already in it.
  useEffect(() => {
    if (!open || !browserRequest) {
      return;
    }
    setUrl(browserRequest.url);
    setHandedOver({
      url: browserRequest.url,
      name: browserRequest.name,
      referer: browserRequest.referer,
    });
    setError(null);
    setTouched(false);
    clearBrowserRequest();
  }, [open, browserRequest, clearBrowserRequest]);

  const address = url.trim();
  const problem = urlProblem(address);
  const valid = problem === null;
  // A hand-over only describes the link it carried, not whatever was pasted
  // over it.
  const handed = handedOver && sameAddress(handedOver.url, address) ? handedOver : null;
  const referer = handed?.referer ?? "";

  // Ask the server what the address holds once it has settled: the name it
  // gives the file, the size, and whether a stop could resume. The probe is
  // advice, not a gate: a server that does not answer still gets a download.
  useEffect(() => {
    if (!open) {
      return;
    }
    setProbe({ kind: "idle" });
    if (!isDesktop() || !downloadUrl(url.trim())) {
      return;
    }
    let active = true;
    let deadline: ReturnType<typeof setTimeout> | undefined;
    setProbe({ kind: "checking" });
    const timer = setTimeout(() => {
      // A server that accepts the connection and then says nothing would keep
      // the check spinning forever. The download can still start, so the
      // dialog says so and stops waiting.
      const tooLong = new Promise<"too long">((resolve) => {
        deadline = setTimeout(() => resolve("too long"), 8_000);
      });
      Promise.race([probeLink(url.trim(), referer), tooLong])
        .then((answer) => {
          if (!active) {
            return;
          }
          if (answer === "too long") {
            setProbe({
              kind: "failed",
              reason:
                "The server did not answer in time. You can still start the download.",
            });
            return;
          }
          setProbe({
            kind: "ready",
            name: answer.name,
            total: answer.total,
            resumable: answer.resumable,
          });
        })
        .catch((failure: unknown) => {
          if (active) {
            setProbe({
              kind: "failed",
              reason: probeWarning(describeError(failure)),
            });
          }
        })
        .finally(() => {
          if (deadline) {
            clearTimeout(deadline);
          }
        });
    }, 450);
    return () => {
      active = false;
      clearTimeout(timer);
      if (deadline) {
        clearTimeout(deadline);
      }
    };
  }, [open, url, referer]);

  // The name and the category follow what was detected until the user picks a
  // category. Nothing here is typed.
  const suggestedName = valid ? safeFileName(fileNameFromUrl(address)) : "";
  const servedName =
    probe.kind === "ready" && probe.name ? safeFileName(probe.name) : null;
  const browserName = handed?.name ? safeFileName(handed.name) : null;
  const name = servedName ?? browserName ?? suggestedName;
  const category =
    categoryOverride ?? (name ? detectCategory(name, categories) : "General");
  const categoryIcon = categories.find(
    (entry) => entry.name === category,
  )?.icon;
  const target =
    folder.trim() && name.trim() ? joinPath(folder.trim(), name.trim()) : "";

  // A category with a folder of its own starts the dialog there. Switching to
  // one without a folder gives the remembered or default folder back.
  useEffect(() => {
    if (!open || folderPicked.current) {
      return;
    }
    const configured = categoryFolder(categories, category);
    categoryFolderApplied.current = configured !== "";
    setFolder(configured || baseFolder.current);
  }, [open, category, categories]);

  // Say so when the exact link is already in the list, without refusing it:
  // a second copy is a legitimate choice.
  const duplicate = useMemo(
    () =>
      valid && allDownloads.some((item) => sameAddress(item.url, address)),
    [valid, allDownloads, address],
  );

  // Announce an existing file before the second click replaces it. The check
  // is a debounce, so the engine is not asked for every letter of the address.
  useEffect(() => {
    setDestination(null);
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
          // A half-typed folder is not an error yet; Download checks again
          // and shows what the engine says.
        });
    }, 400);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [open, target]);

  function reset() {
    setUrl("");
    setHandedOver(null);
    setFolder("");
    setCategoryOverride(null);
    setCategoryAt(null);
    setProbe({ kind: "idle" });
    setDestination(null);
    setTouched(false);
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
        folderPicked.current = true;
        setFolder(picked);
        setError(null);
      }
    } catch (failure) {
      setError(describeError(failure));
    }
  }

  async function submit() {
    if (running.current) {
      return;
    }
    setTouched(true);
    setError(null);

    if (!valid) {
      // The field explains itself; do not say it twice.
      urlField.current?.focus();
      return;
    }
    const chosenName = name.trim();
    if (!chosenName) {
      setError("The file needs a name; the address did not give one.");
      return;
    }
    const chosenFolder = folder.trim();
    if (!chosenFolder) {
      setError("Choose a folder to save the file in.");
      folderButton.current?.focus();
      return;
    }
    if (!isDesktop()) {
      setError("The download engine is only available in the desktop window.");
      return;
    }

    running.current = true;
    setBusy(true);
    try {
      const outcome = await addDownload({
        url: address,
        fileName: chosenName,
        category,
        savePath: chosenFolder,
        referer: referer || undefined,
      });
      // A dismissed destination question leaves the dialog as it was, with
      // the address and the folder still in it.
      if (outcome === "cancelled") {
        return;
      }
      if (outcome === "started") {
        saveLastFolder(chosenFolder);
      }
      onClose();
    } catch (failure) {
      setError(describeError(failure));
      // The engine is the authority on the destination, so it is asked again
      // after a refusal; the hint under the folder follows its answer.
      void inspectDestination(joinPath(chosenFolder, chosenName))
        .then(setDestination)
        .catch(() => undefined);
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  function toggleCategoryMenu() {
    const box = categoryButton.current?.getBoundingClientRect();
    setCategoryAt(
      categoryAt || !box ? null : { x: box.left, y: box.bottom + 4 },
    );
  }

  const categoryRows: MenuRow<CategoryName>[] = categories.map((entry) => ({
    command: entry.name,
    label: entry.name,
    radio: true,
    icon: (
      <FileTypeIcon
        category={entry.name}
        icon={entry.icon}
        className="size-3.5"
      />
    ),
  }));

  const showProblem = touched && problem !== null;
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
  const details = [
    probe.kind === "checking" ? "Checking link…" : sizeLabel,
    resumeLabel,
    duplicate ? "Already in your list" : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <Dialog
      open={open}
      title="Add URL"
      onClose={close}
      dismissible={!busy}
      className="w-[min(30rem,calc(100vw-2rem))]"
      footer={
        <>
          <DialogAction onClick={close} disabled={busy}>
            Cancel
          </DialogAction>
          <DialogAction
            type="submit"
            form="add-url-form"
            tone="primary"
            disabled={busy}
            icon={
              busy ? (
                <SpinnerGlyph className="size-3.5 motion-safe:animate-spin" />
              ) : (
                <DownloadGlyph className="size-3.5" />
              )
            }
          >
            {busy ? "Starting…" : "Download"}
          </DialogAction>
        </>
      }
    >
      <form
        id="add-url-form"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
        aria-busy={busy}
        className="flex flex-col gap-3.5"
        noValidate
      >
        <div>
          <label htmlFor="add-url" className={LABEL}>
            URL
          </label>
          <input
            id="add-url"
            ref={urlField}
            data-autofocus
            className={`${FIELD} h-9 font-mono`}
            value={url}
            disabled={busy}
            spellCheck={false}
            autoComplete="off"
            inputMode="url"
            aria-invalid={showProblem || undefined}
            aria-describedby={showProblem ? "add-url-problem" : undefined}
            onChange={(event) => {
              setUrl(event.target.value);
              setError(null);
            }}
            onBlur={() => {
              if (address) {
                setTouched(true);
              }
            }}
            placeholder="https://example.org/file.zip"
          />
          {showProblem ? (
            <p
              id="add-url-problem"
              role="alert"
              className="mt-1.5 flex items-start gap-1.5 text-chrome-micro text-chrome-error"
            >
              <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
              <span className="min-w-0 break-words">{problem}</span>
            </p>
          ) : null}
        </div>

        {/* What the address adds up to, before anything starts: the file it
            becomes, its size and whether it can be resumed. It is the answer
            to "did I paste the right link"; the category below it. */}
        {valid ? (
          <div className="flex min-w-0 items-center gap-2.5">
            <FileTypeIcon
              category={category}
              icon={categoryIcon}
              className="size-7 shrink-0"
            />
            <div className="min-w-0 flex-1" aria-live="polite">
              <Tooltip content={name} side="top" className="block w-full min-w-0">
                <p className="truncate text-xs text-chrome-title-text">
                  {name}
                </p>
              </Tooltip>
              {details ? (
                <p className="truncate text-chrome-micro text-chrome-muted">
                  {details}
                </p>
              ) : null}
            </div>
          </div>
        ) : null}

        {valid && probe.kind === "failed" ? (
          <p
            role="status"
            className="flex items-start gap-1.5 text-chrome-micro text-chrome-menu-text"
          >
            <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0 break-words">{probe.reason}</span>
          </p>
        ) : null}

        {/* The category is detected from the name, and the picker beside it is
            only there to overrule that. It sits above the folder because the
            category decides which folder the form starts on. */}
        <div>
          <span className={LABEL}>Category</span>
          <button
            ref={categoryButton}
            type="button"
            aria-haspopup="menu"
            aria-expanded={categoryAt !== null}
            aria-label={`Category: ${category}`}
            onClick={toggleCategoryMenu}
            // Keep the menu's own outside-pointer close from firing first, so
            // a second click on this button sees the open state and closes it.
            onPointerDown={(event) => event.stopPropagation()}
            disabled={busy}
            className="inline-flex h-9 items-center gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 text-left text-xs text-chrome-menu-text transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-60 disabled:hover:border-chrome-line"
          >
            <FileTypeIcon
              category={category}
              icon={categoryIcon}
              className="size-3.5 shrink-0"
            />
            <span className="min-w-0 truncate">{category}</span>
            <ChevronDown className="size-3.5 shrink-0 text-chrome-muted" />
          </button>

          {categoryAt ? (
            <MenuPopup
              id="add-url-category"
              label="Choose a category"
              rows={categoryRows}
              level={0}
              focus="first"
              at={categoryAt}
              checked={{ [category]: true }}
              onChoose={(row) => {
                setCategoryOverride(row.command);
                setCategoryAt(null);
                categoryButton.current?.focus();
              }}
              onClose={(reason) => {
                setCategoryAt(null);
                // A click elsewhere moves the focus where the user clicked.
                if (reason !== "pointer") {
                  categoryButton.current?.focus();
                }
              }}
            />
          ) : null}
        </div>

        <div>
          <span className={LABEL}>Save to</span>
          <Tooltip
            content={folder || "Choose a folder"}
            side="top"
            className="block w-full min-w-0"
          >
            <button
              ref={folderButton}
              type="button"
              onClick={() => void pickFolder()}
              disabled={busy}
              aria-label={
                folder
                  ? `Change the save folder, currently ${folder}`
                  : "Choose a save folder"
              }
              className={FOLDER_ROW}
            >
              <FolderGlyph className="size-3.5 shrink-0 text-chrome-muted" />
              <span className="min-w-0 flex-1 truncate font-mono text-chrome-micro text-chrome-menu-text">
                {folder || "Choose a folder"}
              </span>
              <span className="shrink-0 text-chrome-micro text-chrome-muted transition-colors group-hover:text-chrome-menu-text">
                Change
              </span>
            </button>
          </Tooltip>
          {destination && !destination.folderExists ? (
            <p className="mt-1.5 text-chrome-micro text-chrome-muted">
              This folder does not exist yet. It will be created.
            </p>
          ) : null}
          {destination?.fileExists || destination?.active ? (
            <p className="mt-1.5 flex items-start gap-1.5 text-chrome-micro text-chrome-menu-text">
              <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
              <span className="min-w-0 break-words">
                {destination.active ? (
                  <>
                    Another download is already using{` `}
                    <span className="font-mono">{name}</span>.
                  </>
                ) : (
                  <>
                    A file named <span className="font-mono">{name}</span> is
                    already there. You will be asked what to do.
                  </>
                )}
              </span>
            </p>
          ) : null}
        </div>

        {error ? (
          <p
            role="alert"
            className="flex items-start gap-1.5 text-xs text-chrome-error"
          >
            <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0 break-words">{error}</span>
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
