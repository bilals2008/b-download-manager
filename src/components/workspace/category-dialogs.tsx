import { useState } from "react";

import { FolderGlyph } from "@/components/icons/action-icons";
import {
  CategoryFolderIcon,
  WarningMark,
} from "@/components/icons/desktop-icons";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { CategoryIconPicker } from "@/components/workspace/category-icon-picker";
import {
  MAX_CATEGORY_EXTENSIONS,
  categoryDraftProblem,
  invalidCategoryExtensions,
  parseCategoryExtensions,
  type CategoryDefinition,
} from "@/lib/categories";
import { defaultCategoryIcon, type CategoryIcon } from "@/lib/category-icons";
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
  const [icon, setIcon] = useState<CategoryIcon>(
    category?.icon ?? defaultCategoryIcon("Other"),
  );
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function pickFolder() {
    if (!isDesktop()) {
      setProblem("Folders can only be chosen in the desktop app.");
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
      icon,
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
          <CategoryFolderIcon
            category={name.trim() || "Other"}
            icon={icon}
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
            Name and file types are fixed. Folder and icon are yours.
          </p>
        ) : null}

        <CategoryIconPicker
          category={name.trim() || "Other"}
          value={icon}
          onChange={setIcon}
          disabled={busy}
        />

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
            Comma-separated, dot optional, up to {MAX_CATEGORY_EXTENSIONS}.
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
          <p className={HINT}>Empty uses the default save folder.</p>
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
 * Removing a category asks what to do with what it holds. Left alone, its
 * downloads move to Other and every file stays where it is; checked, the
 * finished files leave the disk and the unfinished rows stop and leave the
 * list, so the category really takes its contents with it.
 */
export function RemoveCategoryDialog({
  open,
  category,
  onClose,
}: RemoveCategoryDialogProps) {
  const { counts, folders, removeCategory } = useDownloads();
  const [deleteFiles, setDeleteFiles] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const count = counts[category.name] ?? 0;
  const folderCount = folders[category.name]?.length ?? 0;

  async function confirm() {
    if (busy) {
      return;
    }
    setBusy(true);
    setProblem(null);
    const wrong = await removeCategory(category.name, { deleteFiles });
    if (wrong) {
      setProblem(wrong);
      setBusy(false);
      return;
    }
    onClose();
  }

  return (
    <Dialog
      open={open}
      title="Remove category"
      onClose={onClose}
      dismissible={!busy}
      footer={
        <>
          <DialogAction data-autofocus onClick={onClose} disabled={busy}>
            Cancel
          </DialogAction>
          <DialogAction
            tone="danger"
            onClick={() => void confirm()}
            disabled={busy}
          >
            {busy
              ? "Removing…"
              : deleteFiles
                ? "Delete and remove"
                : "Remove category"}
          </DialogAction>
        </>
      }
    >
      <div className="flex items-center gap-2.5 rounded-chrome border border-chrome-line bg-chrome-canvas p-2.5">
        <CategoryFolderIcon
          category={category.name}
          icon={category.icon}
          className="size-7 shrink-0"
        />
        <div className="min-w-0">
          <p className="truncate text-xs text-chrome-title-text">
            {category.name}
          </p>
          <p className="truncate text-chrome-micro text-chrome-muted">
            {count} {count === 1 ? "download" : "downloads"} · {folderCount}{" "}
            {folderCount === 1 ? "save folder" : "save folders"}
          </p>
        </div>
      </div>

      {count > 0 ? (
        <Checkbox
          className="mt-3"
          label={`Also delete its ${count} ${count === 1 ? "download" : "downloads"}`}
          checked={deleteFiles}
          disabled={busy}
          onChange={(event) => {
            setDeleteFiles(event.target.checked);
            setProblem(null);
          }}
        />
      ) : folderCount > 0 ? (
        <p className="mt-3 text-xs text-chrome-menu-text">
          The save folders stay on disk.
        </p>
      ) : null}

      {problem ? (
        <p
          role="alert"
          className="mt-3 flex items-start gap-1.5 text-xs text-chrome-error"
        >
          <WarningMark tone="red" className="mt-px size-3.5 shrink-0" />
          <span className="min-w-0 break-words">{problem}</span>
        </p>
      ) : null}
    </Dialog>
  );
}
