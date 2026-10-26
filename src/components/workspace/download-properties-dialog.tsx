import { useRef, useState } from "react";

import { CopyGlyph, FolderGlyph } from "@/components/icons/action-icons";
import { WarningMark } from "@/components/icons/desktop-icons";
import { SpinnerGlyph } from "@/components/icons/ui-icons";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { ListBox } from "@/components/ui/listbox";
import { Tooltip } from "@/components/ui/tooltip";
import { copyText } from "@/lib/clipboard";
import { isStartRefusal } from "@/lib/conflict";
import {
  destinationLock,
  fileNameProblem,
  formatDate,
  formatSize,
  joinPath,
  progressOf,
  resumableLabel,
  STATUS_LABELS,
  type CategoryName,
  type Download,
} from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import { chooseFolder, type ConflictChoice, type StartRefusal } from "@/lib/ipc";
import { useDownloads } from "@/state/download-store";

const LABEL = "mb-1.5 block text-xs text-chrome-muted";

const FOLDER_ROW =
  "group flex h-9 w-full min-w-0 items-center gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 text-left transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-60 disabled:hover:border-chrome-line";

interface DownloadPropertiesDialogProps {
  item: Download;
  onClose: () => void;
}

/**
 * The properties of one download, the way IDM shows them.
 *
 * The category and the note are metadata, so they can be saved while the
 * download runs. The name and the folder move the file itself, which a task
 * still writing cannot do: the fields are held until it stops. A file already
 * at the new name is the engine's question, answered here rather than in a
 * second window.
 */
export function DownloadPropertiesDialog({
  item,
  onClose,
}: DownloadPropertiesDialogProps) {
  const { categories, changeDestination, reportError, updateMeta } =
    useDownloads();
  const locked = destinationLock(item);
  const [name, setName] = useState(item.fileName);
  const [folder, setFolder] = useState(item.savePath);
  const [category, setCategory] = useState<CategoryName>(item.category);
  const [description, setDescription] = useState(item.description ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<StartRefusal | null>(null);
  // A second click in the same tick reaches the command before React has
  // re-rendered the disabled button; this is the guard that holds.
  const running = useRef(false);

  const chosenName = name.trim();
  const chosenFolder = folder.trim();
  const nameProblem = fileNameProblem(chosenName);
  const target = chosenName && chosenFolder ? joinPath(chosenFolder, chosenName) : "";

  const categoryChanged = category !== item.category;
  const descriptionChanged = description.trim() !== (item.description ?? "");
  const nameChanged = chosenName !== item.fileName;
  // Compare the joined path, so a folder picked with a trailing separator is
  // not a change and never reaches the engine's conflict check.
  const locationChanged = target !== "" && target !== item.dest;
  const dirty = categoryChanged || descriptionChanged || locationChanged;
  const canSave = dirty && !busy && nameProblem === null && chosenFolder !== "";

  async function copyAddress() {
    if (!(await copyText(item.url))) {
      reportError("The address could not be copied to the clipboard.");
    }
  }

  async function pickFolder() {
    if (!isDesktop()) {
      setError("The folder picker is only available in the desktop window.");
      return;
    }
    try {
      const picked = await chooseFolder(chosenFolder);
      if (picked) {
        setFolder(picked);
        setConflict(null);
        setError(null);
      }
    } catch (failure) {
      setError(describeError(failure));
    }
  }

  /**
   * Save the fields. The metadata goes first, so it is not lost when the new
   * location turns out to be taken; `choice` settles that question.
   */
  async function save(choice: ConflictChoice = "ask") {
    if (running.current) {
      return;
    }
    setError(null);
    if (nameProblem !== null) {
      setError(nameProblem);
      return;
    }
    if (!chosenFolder) {
      setError("Choose a folder to save the file in.");
      return;
    }

    running.current = true;
    setBusy(true);
    try {
      if (categoryChanged || descriptionChanged) {
        await updateMeta(
          item.id,
          category,
          description.trim() || null,
        );
      }
      if (locationChanged) {
        await changeDestination(item.id, target, choice);
      }
      onClose();
    } catch (failure) {
      if (
        isStartRefusal(failure) &&
        (failure.kind === "exists" || failure.kind === "active")
      ) {
        setConflict(failure);
        return;
      }
      setError(describeError(failure));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  const percent = progressOf(item);

  return (
    <Dialog
      open
      title="Properties"
      onClose={busy ? () => undefined : onClose}
      dismissible={!busy}
      className="max-h-[calc(100vh-2rem)] w-[min(38rem,calc(100vw-2rem))]"
      bodyClassName="overflow-y-auto scrollbar-thin"
      footer={
        <>
          <DialogAction
            className="mr-auto"
            icon={<CopyGlyph className="size-3.5" />}
            onClick={() => void copyAddress()}
            disabled={busy}
          >
            Copy address
          </DialogAction>
          <DialogAction onClick={onClose} disabled={busy}>
            {dirty ? "Cancel" : "Close"}
          </DialogAction>
          <DialogAction
            type="submit"
            form="download-properties-form"
            tone="primary"
            disabled={!canSave}
            icon={
              busy ? (
                <SpinnerGlyph className="size-3.5 motion-safe:animate-spin" />
              ) : undefined
            }
          >
            {busy ? "Saving…" : "Save"}
          </DialogAction>
        </>
      }
    >
      <form
        id="download-properties-form"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        aria-busy={busy}
        className="flex flex-col gap-3.5"
        noValidate
      >
        <div>
          <label htmlFor="download-properties-name" className={LABEL}>
            File name
          </label>
          <input
            id="download-properties-name"
            data-autofocus={locked ? undefined : true}
            className={`${FIELD} h-9`}
            value={name}
            disabled={busy || locked !== null}
            spellCheck={false}
            autoComplete="off"
            aria-invalid={nameProblem !== null && nameChanged ? true : undefined}
            aria-describedby={
              nameProblem !== null && nameChanged
                ? "download-properties-name-problem"
                : undefined
            }
            onChange={(event) => {
              setName(event.target.value);
              setConflict(null);
              setError(null);
            }}
          />
          {nameProblem !== null && nameChanged ? (
            <p
              id="download-properties-name-problem"
              role="alert"
              className="mt-1.5 flex items-start gap-1.5 text-chrome-micro text-chrome-error"
            >
              <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
              <span className="min-w-0 break-words">{nameProblem}</span>
            </p>
          ) : null}
        </div>

        <div>
          <span className={LABEL}>Save to</span>
          <Tooltip
            content={chosenFolder || "Choose a folder"}
            side="top"
            className="block w-full min-w-0"
          >
            <button
              type="button"
              onClick={() => void pickFolder()}
              disabled={busy || locked !== null}
              aria-label={
                chosenFolder
                  ? `Change the save folder, currently ${chosenFolder}`
                  : "Choose a save folder"
              }
              className={FOLDER_ROW}
            >
              <FolderGlyph className="size-3.5 shrink-0 text-chrome-muted" />
              <span className="min-w-0 flex-1 truncate font-mono text-chrome-micro text-chrome-menu-text">
                {chosenFolder || "Choose a folder"}
              </span>
              <span className="shrink-0 text-chrome-micro text-chrome-muted transition-colors group-hover:text-chrome-menu-text">
                Change
              </span>
            </button>
          </Tooltip>
          <Tooltip content={target} side="bottom" className="block w-full min-w-0">
            <p className="mt-1 truncate font-mono text-chrome-micro text-chrome-muted">
              {target}
            </p>
          </Tooltip>
          {locked ? (
            <p className="mt-1.5 text-chrome-micro text-chrome-muted">{locked}</p>
          ) : !nameProblem && item.status !== "completed" ? (
            <p className="mt-1.5 text-chrome-micro text-chrome-muted">
              If the name is taken, this download is saved beside the other file
              under a free name.
            </p>
          ) : null}
        </div>

        <div>
          <label htmlFor="download-properties-description" className={LABEL}>
            Description <span className="text-chrome-muted">(optional)</span>
          </label>
          <textarea
            id="download-properties-description"
            data-autofocus={locked ? true : undefined}
            className={`${FIELD} min-h-16 resize-y`}
            value={description}
            disabled={busy}
            rows={3}
            onChange={(event) => {
              setDescription(event.target.value);
              setError(null);
            }}
          />
        </div>

        <div>
          <span className={LABEL}>Category</span>
          <ListBox
            label="Category"
            value={category}
            disabled={busy}
            options={categories.map((entry) => ({
              value: entry.name,
              label: entry.name,
            }))}
            onChange={(value) => {
              setCategory(value as CategoryName);
              setError(null);
            }}
          />
        </div>

        {conflict ? (
          <div
            role="alert"
            className="rounded-chrome bg-chrome-canvas px-2.5 py-2 text-xs text-chrome-menu-text"
          >
            <p className="flex items-start gap-1.5">
              <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
              <span className="min-w-0 break-words">{conflict.message}</span>
            </p>
            <p className="mt-1 font-mono text-chrome-micro break-all text-chrome-muted">
              {conflict.path}
            </p>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              {conflict.kind === "exists" ? (
                <DialogAction
                  tone="danger"
                  disabled={busy}
                  onClick={() => void save("replace")}
                >
                  Replace
                </DialogAction>
              ) : null}
              <DialogAction
                tone="secondary"
                disabled={busy}
                onClick={() => void save("rename")}
              >
                Keep both
              </DialogAction>
            </div>
            <p className="mt-1.5 text-chrome-micro text-chrome-muted">
              {conflict.kind === "exists"
                ? "Replace overwrites the file at the new location. Keep both saves this download beside it under a free name."
                : "Keep both saves this download beside it under a free name."}
            </p>
          </div>
        ) : null}

        {error ? (
          <p
            role="alert"
            className="flex items-start gap-1.5 text-xs text-chrome-error"
          >
            <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0 break-words">{error}</span>
          </p>
        ) : null}

        <div className="border-t border-chrome-line pt-3.5">
          <h3 className="mb-2 text-xs font-medium text-chrome-title-text">
            Details
          </h3>
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-start gap-x-4 gap-y-1.5 text-xs">
            <Field label="Address">
              <span className="font-mono break-all">{item.url}</span>
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
            <Field label="Connections">{item.connections}</Field>
            <Field label="Resume">{resumableLabel(item.resumable)}</Field>
            <Field label="Added">{formatDate(item.addedAt)}</Field>
            <Field label="Last try">{formatDate(item.lastTryAt)}</Field>
            {item.error && item.status === "error" ? (
              <Field label="Error">
                <span className="text-chrome-error">{item.error}</span>
              </Field>
            ) : null}
          </dl>
        </div>
      </form>
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
