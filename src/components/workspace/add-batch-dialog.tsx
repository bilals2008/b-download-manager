import { DownloadGlyph, SpinnerGlyph } from "@/components/icons/ui-icons";
import { FolderGlyph } from "@/components/icons/action-icons";
import { WarningMark } from "@/components/icons/desktop-icons";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { Tooltip } from "@/components/ui/tooltip";
import { useBatchSeed } from "@/components/workspace/batch-seed";
import { useEffect, useMemo, useRef, useState } from "react";

import { detectCategory } from "@/lib/categories";
import { PREVIEW_LIMIT, expandBatch, type BatchPattern } from "@/lib/batch";
import { fileNameFromUrl, safeFileName, splitPath } from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import { chooseFolder, defaultDestination } from "@/lib/ipc";
import { preferredFolder, saveLastFolder } from "@/lib/settings";
import { useDownloads } from "@/state/download-store";

interface AddBatchDialogProps {
  open: boolean;
  /** The pattern to open with, from the clipboard or from the menu. */
  seed?: BatchPattern | null;
  onClose: () => void;
}

const LABEL = "mb-1.5 block text-xs text-chrome-muted";

const FOLDER_ROW =
  "group flex h-9 w-full min-w-0 items-center gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 text-left transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-60 disabled:hover:border-chrome-line";

const RANGE_FIELD = `${FIELD} h-9 w-20 text-center font-mono tabular-nums`;

/** One of two choices, drawn as a pair of buttons rather than a menu. */
function Segmented({
  label,
  value,
  onChange,
  disabled,
}: {
  label: string;
  value: "numbers" | "letters";
  onChange: (value: "numbers" | "letters") => void;
  disabled?: boolean;
}) {
  const options: { value: "numbers" | "letters"; label: string }[] = [
    { value: "numbers", label: "Numbers" },
    { value: "letters", label: "Letters" },
  ];
  return (
    <div
      role="group"
      aria-label={label}
      className="inline-flex rounded-chrome border border-chrome-line bg-chrome-canvas p-0.5"
    >
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={active}
            disabled={disabled}
            onClick={() => onChange(option.value)}
            className={
              active
                ? "h-7 rounded-chrome bg-primary px-2.5 text-xs text-primary-foreground"
                : "h-7 rounded-chrome px-2.5 text-xs text-chrome-menu-text transition-colors hover:bg-chrome-menu-hover disabled:opacity-60"
            }
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

/**
 * "Add batch download": one address with a `*`, and the range it stands for.
 *
 * The list is built as the reader types and shown before anything starts, so
 * a wrong range is a correction rather than a folder full of the wrong files.
 * Every address starts as an ordinary download, in the folder chosen here and
 * in the category its own name belongs to.
 */
export function AddBatchDialog({ open, seed = null, onClose }: AddBatchDialogProps) {
  const { addDownload, categories } = useDownloads();
  // The clipboard answer can arrive after the dialog is open; the hook keeps
  // it from replacing what the reader has already written.
  const { pattern, change: changePattern } = useBatchSeed(open, seed);
  const [folder, setFolder] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(
    null,
  );
  // A second press in the same tick reaches the command before React has
  // re-rendered the disabled button; this is the guard that holds.
  const running = useRef(false);

  useEffect(() => {
    if (!open) {
      return;
    }
    setFolder(preferredFolder() ?? "");
    setError(null);
    setBusy(false);
    setProgress(null);
    let live = true;
    // No folder remembered yet: the one the engine would save to, so the
    // first batch lands where every other download does.
    if (!preferredFolder()) {
      void defaultDestination()
        .then((answer) => {
          const { dir } = splitPath(answer);
          if (live && dir && dir !== ".") {
            setFolder((current) => current || dir);
          }
        })
        .catch(() => {
          // The reader can still pick a folder by hand.
        });
    }
    return () => {
      live = false;
    };
  }, [open]);

  const { urls, problem } = useMemo(() => expandBatch(pattern), [pattern]);
  const canStart = urls.length > 0 && problem === null && !busy;
  const blankAddress = pattern.address.trim() === "";
  const shown = urls.slice(0, PREVIEW_LIMIT);
  const remaining = urls.length - shown.length;

  function change(update: Partial<BatchPattern>) {
    changePattern(update);
    setError(null);
  }

  async function pickFolder() {
    if (!isDesktop()) {
      setError("Folders can only be chosen in the desktop app.");
      return;
    }
    try {
      const picked = await chooseFolder(folder.trim());
      if (picked) {
        setFolder(picked);
        setError(null);
      }
    } catch (failure) {
      setError(describeError(failure));
    }
  }

  async function submit() {
    if (running.current || !canStart) {
      return;
    }
    const target = folder.trim();
    if (!target) {
      setError("Choose a folder to save the files in.");
      return;
    }
    if (!isDesktop()) {
      setError("Downloading works only in the desktop app.");
      return;
    }

    running.current = true;
    setBusy(true);
    setError(null);
    let added = 0;
    try {
      for (const [index, url] of urls.entries()) {
        setProgress({ done: index, total: urls.length });
        const name =
          safeFileName(fileNameFromUrl(url)) || `download-${index + 1}`;
        const outcome = await addDownload({
          url,
          fileName: name,
          category: detectCategory(name, categories),
          savePath: target,
        });
        // A dismissed question is the reader's answer: stop here, keep what
        // was already added, and do not ask it eleven more times.
        if (outcome === "cancelled") {
          break;
        }
        if (outcome === "started") {
          added += 1;
        }
      }
      if (added > 0) {
        saveLastFolder(target);
      }
      onClose();
    } catch (failure) {
      setError(
        added > 0
          ? `Added ${added} of ${urls.length}, then stopped: ${describeError(failure)}`
          : describeError(failure),
      );
    } finally {
      running.current = false;
      setBusy(false);
      setProgress(null);
    }
  }

  return (
    <Dialog
      open={open}
      title="Add batch download"
      onClose={onClose}
      dismissible={!busy}
      className="w-[min(32rem,calc(100vw-2rem))]"
      footer={
        <>
          <DialogAction tone="quiet" disabled={busy} onClick={onClose}>
            Cancel
          </DialogAction>
          <DialogAction
            type="submit"
            form="add-batch-form"
            tone="primary"
            disabled={!canStart}
            icon={
              busy ? (
                <SpinnerGlyph className="size-3.5 motion-safe:animate-spin" />
              ) : (
                <DownloadGlyph className="size-3.5" />
              )
            }
          >
            {busy && progress
              ? `Adding ${Math.min(progress.done + 1, progress.total)} of ${progress.total}…`
              : `Download${urls.length > 0 ? ` ${urls.length}` : ""}`}
          </DialogAction>
        </>
      }
    >
      <form
        id="add-batch-form"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
        aria-busy={busy}
        className="flex flex-col gap-3.5"
        noValidate
      >
        <div>
          <label htmlFor="batch-address" className={LABEL}>
            Address
          </label>
          <input
            id="batch-address"
            data-autofocus
            className={`${FIELD} h-9 font-mono`}
            value={pattern.address}
            disabled={busy}
            spellCheck={false}
            autoComplete="off"
            inputMode="url"
            aria-invalid={(!blankAddress && problem !== null) || undefined}
            aria-describedby={problem ? "batch-problem" : undefined}
            onChange={(event) => change({ address: event.target.value })}
            placeholder="https://example.org/poster_*.jpg"
          />
          <p className="mt-1.5 text-chrome-micro text-chrome-muted">
            The <span className="font-mono">*</span> is replaced by each entry of
            the range below.
          </p>
        </div>

        <div className="flex flex-wrap items-end gap-3">
          <div>
            <span className={LABEL}>Replace with</span>
            <Segmented
              label="Replace the * with"
              value={pattern.letters ? "letters" : "numbers"}
              disabled={busy}
              onChange={(value) =>
                change({
                  letters: value === "letters",
                  from: value === "letters" ? "a" : "1",
                  to: value === "letters" ? "c" : "10",
                })
              }
            />
          </div>
          <div>
            <label htmlFor="batch-from" className={LABEL}>
              From
            </label>
            <input
              id="batch-from"
              className={RANGE_FIELD}
              value={pattern.from}
              disabled={busy}
              spellCheck={false}
              autoComplete="off"
              maxLength={pattern.letters ? 1 : 9}
              onChange={(event) => change({ from: event.target.value })}
            />
          </div>
          <div>
            <label htmlFor="batch-to" className={LABEL}>
              To
            </label>
            <input
              id="batch-to"
              className={RANGE_FIELD}
              value={pattern.to}
              disabled={busy}
              spellCheck={false}
              autoComplete="off"
              maxLength={pattern.letters ? 1 : 9}
              onChange={(event) => change({ to: event.target.value })}
            />
          </div>
          {pattern.letters ? null : (
            <div>
              <label htmlFor="batch-padding" className={LABEL}>
                Digits
              </label>
              <input
                id="batch-padding"
                className={RANGE_FIELD}
                value={pattern.padding}
                disabled={busy}
                spellCheck={false}
                autoComplete="off"
                inputMode="numeric"
                maxLength={2}
                onChange={(event) => change({ padding: event.target.value })}
              />
            </div>
          )}
        </div>

        {problem && !blankAddress ? (
          <p
            id="batch-problem"
            role="alert"
            className="flex items-start gap-1.5 text-chrome-micro text-chrome-error"
          >
            <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0 break-words">{problem}</span>
          </p>
        ) : null}

        <div className="rounded-chrome bg-chrome-canvas px-3 py-2.5">
          <p
            className="mb-1.5 text-chrome-micro text-chrome-muted"
            aria-live="polite"
          >
            {urls.length === 0 ? (
              "No addresses yet"
            ) : (
              <>
                <span className="tabular-nums">{urls.length}</span>{" "}
                {urls.length === 1 ? "address" : "addresses"}
              </>
            )}
          </p>
          {shown.length === 0 ? (
            <p className="text-chrome-micro text-chrome-muted">
              Write the address with a <span className="font-mono">*</span> where
              the number or the letter goes.
            </p>
          ) : (
            <ul className="flex flex-col gap-1">
              {shown.map((url) => (
                <li key={url} className="min-w-0">
                  <Tooltip
                    content={url}
                    side="top"
                    className="block w-full min-w-0"
                  >
                    <span className="block truncate font-mono text-chrome-micro text-chrome-menu-text">
                      {url}
                    </span>
                  </Tooltip>
                </li>
              ))}
              {remaining > 0 ? (
                <li className="text-chrome-micro text-chrome-muted">
                  and <span className="tabular-nums">{remaining}</span> more
                </li>
              ) : null}
            </ul>
          )}
        </div>

        <div>
          <span className={LABEL}>Save to</span>
          <Tooltip
            content={folder || "Choose a folder"}
            side="top"
            className="block w-full min-w-0"
          >
            <button
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
          <p className="mt-1.5 text-chrome-micro text-chrome-muted">
            Each file goes to its own category inside this folder.
          </p>
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
