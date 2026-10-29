import { useState } from "react";

import { FileTypeIcon, WarningMark } from "@/components/icons/desktop-icons";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { Tooltip } from "@/components/ui/tooltip";
import {
  answerAllowed,
  applyToRemainingLabel,
  canApplyToRemaining,
  type ConflictAnswer,
  type FileConflict,
} from "@/lib/conflict";
import { categoryOfFile } from "@/lib/downloads";
import { useDownloads } from "@/state/download-store";

interface FileExistsDialogProps {
  conflict: FileConflict;
  /** The answer, and whether it covers the rest of a batch. */
  onAnswer: (answer: ConflictAnswer, applyToRemaining: boolean) => void;
  /** The question was dismissed; nothing is started. */
  onCancel: () => void;
}

/**
 * The File Exists window: what happens when the name a download wants is
 * already taken.
 *
 * Four answers, one line each: keep both files by saving under a free name,
 * replace the file after looking at it, skip this download, or cancel the
 * whole thing. The name and the folder are the ones the engine refused, so the
 * file on screen is the file that was in the way. The safe answer — a new
 * name — has the accent and the focus, and nothing touches the disk until one
 * of them is pressed.
 */
export function FileExistsDialog({
  conflict,
  onAnswer,
  onCancel,
}: FileExistsDialogProps) {
  const { categories } = useDownloads();
  const category = categoryOfFile(conflict.name, categories);
  const [applyToRemaining, setApplyToRemaining] = useState(false);
  const batch = canApplyToRemaining(conflict);
  const name = conflict.suggestedName;

  function answer(choice: ConflictAnswer) {
    onAnswer(choice, batch && applyToRemaining);
  }

  return (
    <Dialog
      open
      title={conflict.active ? "File is in use" : "File already exists"}
      onClose={onCancel}
      className="w-[min(26rem,calc(100vw-2rem))]"
      footer={
        <>
          <DialogAction tone="quiet" onClick={onCancel}>
            Cancel
          </DialogAction>
          <DialogAction onClick={() => answer("skip")}>Skip</DialogAction>
          <DialogAction
            tone="danger"
            disabled={!answerAllowed(conflict, "replace")}
            onClick={() => answer("replace")}
          >
            Replace
          </DialogAction>
          <DialogAction tone="primary" data-autofocus onClick={() => answer("rename")}>
            Save as new name
          </DialogAction>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <FileTypeIcon
            category={category}
            icon={categories.find((entry) => entry.name === category)?.icon}
            name={conflict.name}
            className="mt-px size-6 shrink-0"
          />
          <div className="min-w-0 flex-1">
            <Tooltip content={conflict.name} side="top" className="block min-w-0">
              <p className="truncate text-xs text-chrome-title-text">
                {conflict.name}
              </p>
            </Tooltip>
            <Tooltip content={conflict.folder} side="top" className="block min-w-0">
              <p className="truncate font-mono text-chrome-micro text-chrome-muted">
                {conflict.folder}
              </p>
            </Tooltip>
          </div>
        </div>

        <p className="flex items-start gap-1.5 text-xs text-chrome-menu-text">
          <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
          <span className="min-w-0">
            {conflict.active
              ? "Another download is already using this file."
              : "A file with this name is already in this folder."}
          </span>
        </p>

        {name ? (
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-3 text-chrome-micro">
            <dt className="text-chrome-muted">Keep as</dt>
            <dd className="min-w-0 truncate font-mono text-chrome-menu-text">
              {name}
            </dd>
          </dl>
        ) : null}

        {conflict.active ? (
          <p className="text-chrome-micro text-chrome-muted">
            Replacing is not possible while another download uses it.
          </p>
        ) : null}

        {batch ? (
          <label className="flex items-center gap-2 text-chrome-micro text-chrome-menu-text">
            <input
              type="checkbox"
              checked={applyToRemaining}
              onChange={(event) => setApplyToRemaining(event.target.checked)}
              className="size-3.5 shrink-0 accent-primary"
            />
            {applyToRemainingLabel(conflict.remaining)}
          </label>
        ) : null}
      </div>
    </Dialog>
  );
}
