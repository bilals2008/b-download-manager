import { useState } from "react";

import { FolderGlyph } from "@/components/icons/action-icons";
import { FileTypeIcon, WarningMark } from "@/components/icons/desktop-icons";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import {
  MAX_CATEGORY_EXTENSIONS,
  categoryDraftProblem,
  invalidCategoryExtensions,
  parseCategoryExtensions,
  type CategoryDefinition,
} from "@/lib/categories";
import { describeError, isDesktop } from "@/lib/errors";
import { chooseFolder, folderStatus } from "@/lib/ipc";
import { useDownloads } from "@/state/download-store";

const LABEL = "mb-1.5 block text-xs text-chrome-muted";
const HINT = "mt-1.5 text-chrome-micro text-chrome-muted";

interface CategoryEditorDialogProps {
  open: boolean;
  /** The category being changed, or null to make a new one. */
  category: CategoryDefinition | null;
  onClose: () => void;
}

/**
 * Add a category or change one.
 *
 * A category is a name, the file types that detect it and the folder its
 * downloads save to. A built-in keeps its name and its types: only the folder
 * is the user's to change. The folder does not have to exist yet, but one that
 * exists and cannot be written to is refused before anything is saved.
 */
export function CategoryEditorDialog({
  open,
  category,
  onClose,
}: CategoryEditorDialogProps) {
  const { categories, addCategory, updateCategory } = useDownloads();
  const builtin = category?.builtin === true;
  const [name, setName] = useState(category?.name ?? "");
  const [extensions, setExtensions] = useState(
    category?.extensions.join(", ") ?? "",
  );
  const [folder, setFolder] = useState(category?.folder ?? "");
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function pickFolder() {
    if (!isDesktop()) {
      setProblem("The folder picker is only available in the desktop window.");
      return;
    }
    try {
      const picked = await chooseFolder(folder.trim());
      if (picked) {
        setFolder(picked);
        setProblem(null);
      }
    } catch (failure) {
      setProblem(describeError(failure));
    }
  }

  async function submit() {
    if (busy) {
      return;
    }
    const invalid = invalidCategoryExtensions(extensions);
    if (invalid.length > 0) {
      setProblem(
        `Not a file type: ${invalid.join(", ")}. Use letters and digits, without the dot.`,
      );
      return;
    }
    const draft = {
      name,
      extensions: parseCategoryExtensions(extensions),
      folder,
    };
    const wrong = category
      ? categoryDraftProblem(draft, categories, category.name)
      : categoryDraftProblem(draft, categories);
    if (wrong) {
      setProblem(wrong);
      return;
    }

    setBusy(true);
    setProblem(null);
    try {
      // A folder that is there and refuses a write is a real fault. A folder
      // that is not there yet is fine: the engine creates it on the first file.
      if (folder.trim() && isDesktop()) {
        const status = await folderStatus(folder.trim());
        if (status.exists && !status.writable) {
          setProblem(status.error ?? "This folder cannot be written to.");
          return;
        }
      }
      const error = category
        ? updateCategory(category.name, draft)
        : addCategory(draft);
      if (error) {
        setProblem(error);
        return;
      }
      onClose();
    } catch (failure) {
      setProblem(describeError(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      open={open}
      title={category ? `${category.name} properties` : "Add category"}
      onClose={onClose}
      dismissible={!busy}
      className="w-[min(28rem,calc(100vw-2rem))]"
      footer={
        <>
          <DialogAction onClick={onClose} disabled={busy}>
            Cancel
          </DialogAction>
          <DialogAction
            tone="primary"
            onClick={() => void submit()}
            disabled={busy}
          >
            {busy ? "Saving…" : category ? "Save" : "Add category"}
          </DialogAction>
        </>
      }
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
        aria-busy={busy}
        className="flex flex-col gap-3.5"
        noValidate
      >
        <div className="flex items-end gap-2.5">
          <FileTypeIcon
            category={name.trim() || "Other"}
            className="size-8 shrink-0"
          />
          <div className="min-w-0 flex-1">
            <label htmlFor="category-name" className={LABEL}>
              Name
            </label>
            <input
              id="category-name"
              data-autofocus={builtin ? undefined : true}
              className={`${FIELD} h-8`}
              value={name}
              disabled={busy || builtin}
              maxLength={40}
              autoComplete="off"
              onChange={(event) => {
                setName(event.target.value);
                setProblem(null);
              }}
            />
          </div>
        </div>
        {builtin ? (
          <p className={HINT}>
            A built-in category keeps its name and its file types. Its folder is
            yours to change.
          </p>
        ) : null}

        <div>
          <label htmlFor="category-extensions" className={LABEL}>
            File types
          </label>
          <input
            id="category-extensions"
            className={`${FIELD} h-8 font-mono`}
            value={extensions}
            disabled={busy || builtin}
            placeholder="pdf, docx, epub"
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => {
              setExtensions(event.target.value);
              setProblem(null);
            }}
          />
          <p className={HINT}>
            Separate the types with commas. A type may be written with or
            without its dot. Up to {MAX_CATEGORY_EXTENSIONS} types.
          </p>
        </div>

        <div>
          <label htmlFor="category-folder" className={LABEL}>
            Save folder
          </label>
          <div className="flex items-center gap-2">
            <input
              id="category-folder"
              data-autofocus={builtin ? true : undefined}
              className={`${FIELD} h-8 min-w-0 flex-1 font-mono`}
              value={folder}
              disabled={busy}
              spellCheck={false}
              autoComplete="off"
              placeholder="The default save folder"
              onChange={(event) => {
                setFolder(event.target.value);
                setProblem(null);
              }}
            />
            <DialogAction
              onClick={() => void pickFolder()}
              disabled={busy}
              icon={<FolderGlyph className="size-3.5" />}
            >
              Browse
            </DialogAction>
          </div>
          <p className={HINT}>
            Leave this empty to follow the default save folder. The folder is
            created with the first file that lands in it.
          </p>
        </div>

        {problem ? (
          <p
            role="alert"
            className="flex items-start gap-1.5 text-xs text-chrome-error"
          >
            <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0 break-words">{problem}</span>
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

interface RemoveCategoryDialogProps {
  open: boolean;
  category: CategoryDefinition;
  onClose: () => void;
}

/**
 * Removing a category moves its downloads to Other. The files stay where they
 * are, so the question says so before the press.
 */
export function RemoveCategoryDialog({
  open,
  category,
  onClose,
}: RemoveCategoryDialogProps) {
  const { counts, removeCategory, reportError } = useDownloads();
  const count = counts[category.name] ?? 0;

  function confirm() {
    const problem = removeCategory(category.name);
    if (problem) {
      reportError(problem);
      return;
    }
    onClose();
  }

  return (
    <Dialog
      open={open}
      title="Remove category"
      onClose={onClose}
      footer={
        <>
          <DialogAction data-autofocus onClick={onClose}>
            Cancel
          </DialogAction>
          <DialogAction tone="danger" onClick={confirm}>
            Remove category
          </DialogAction>
        </>
      }
    >
      <p className="text-xs text-chrome-menu-text">
        Remove {category.name}?{" "}
        {count > 0
          ? `${count === 1 ? "Its download moves" : `Its ${count} downloads move`} to Other.`
          : "No downloads are in it."}{" "}
        Files already saved to disk are not deleted or moved.
      </p>
    </Dialog>
  );
}
