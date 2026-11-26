import { FolderGlyph } from "@/components/icons/action-icons";
import {
  FileTypeIcon,
  WarningMark,
} from "@/components/icons/desktop-icons";
import {
  ChevronDown,
  DownloadGlyph,
  SpinnerGlyph,
} from "@/components/icons/ui-icons";
import { MenuPopup } from "@/components/shell/menu-popup";
import { Dialog } from "@/components/ui/dialog";
import { ListBox } from "@/components/ui/listbox";
import { DialogAction } from "@/components/ui/dialog-action";
import { Switch } from "@/components/ui/switch";
import { FIELD } from "@/components/ui/field";
import { FormatCheckList, FormatList } from "@/components/ui/quality-list";
import { Tooltip } from "@/components/ui/tooltip";
import { driveExports } from "@/lib/drive";
import { formatDuration, type QualityRow } from "@/lib/media";
import { useAddDownloadForm } from "@/components/workspace/use-add-download-form";

interface AddDownloadDialogProps {
  open: boolean;
  onClose: () => void;
  /** An address to open with, from a link watched on the clipboard. */
  seedUrl?: string | null;
  /** Hand a magnet link or a `.torrent` to the torrent dialog, which lists
   * its files before anything is written. */
  onTorrent?: (source: string) => void;
}

const LABEL = "mb-1.5 block text-xs text-chrome-muted";

/** The word beside one media row. */
const MEDIA_KIND: Record<string, string> = {
  video: "Video",
  audio: "Audio",
};

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
 * The model behind the form is `useAddDownloadForm`: this file is the form. A
 * dialog that owns its state and shows it in one place is the file that grows
 * without a limit, so what the address means lives next to the code that
 * decides it, and only the layout is left here.
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
  onTorrent,
  seedUrl,
}: AddDownloadDialogProps) {
  const {
    url,
    setUrl,
    folder,
    setCategoryOverride,
    categoryAt,
    setCategoryAt,
    probe,
    gallery,
    debridAvailable,
    debridUnlock,
    setDebridUnlock,
    destination,
    error,
    setError,
    setTouched,
    busy,
    urlField,
    folderButton,
    categoryButton,
    firstRunNotice,
    address,
    torrent,
    problem,
    valid,
    drive,
    driveDocument,
    driveExport,
    setDriveExport,
    name,
    category,
    categoryIcon,
    close,
    pickFolder,
    submit,
    downloadGallery,
    media,
    mediaChoice,
    toggleMedia,
    videoPage,
    video,
    qualities,
    qualityChoice,
    chooseQuality,
    qualityError,
    downloadMediaFiles,
    toggleCategoryMenu,
    categoryRows,
    showProblem,
    details,
  } = useAddDownloadForm({ open, onClose, seedUrl });

  // The files a page names, drawn as the same rows as its qualities.
  const fileRows: QualityRow[] = (media?.files ?? []).map((file) => ({
    id: file.url,
    label: file.name,
    note: MEDIA_KIND[file.kind] ?? null,
    size: null,
    audio: null,
    kind: file.kind,
    height: null,
    bandwidth: null,
  }));

  const qualitySummary = (() => {
    if (qualityError) {
      return "The page's video could not be read.";
    }
    if (!qualities.length) {
      return video?.state === "ready"
        ? "This page offers no downloadable quality."
        : "Reading the page's video…";
    }
    const count =
      qualities.length === 1 ? "1 quality" : `${qualities.length} qualities`;
    const duration = formatDuration(video?.duration ?? null);
    return [video?.title?.trim() || null, count, duration || null]
      .filter(Boolean)
      .join(" · ");
  })();

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
          {torrent ? (
            <DialogAction
              tone="primary"
              disabled={busy}
              onClick={() => onTorrent?.(address)}
            >
              Add torrent
            </DialogAction>
          ) : (
            <DialogAction
              type="submit"
              form="add-url-form"
              tone="primary"
              disabled={busy || drive?.kind === "folder"}
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
          )}
        </>
      }
    >
      <form
        id="add-url-form"
        onSubmit={(event) => {
          event.preventDefault();
          // Enter on a magnet link opens the torrent dialog, the one place a
          // torrent's files are chosen; submitting here would do nothing.
          if (torrent) {
            onTorrent?.(address);
            return;
          }
          void submit();
        }}
        aria-busy={busy}
        className="flex flex-col gap-3.5"
        noValidate
      >
        {/* The one first-run sentence: where the files land while no save
            folder has been chosen. It never returns after this opening. */}
        {firstRunNotice ? (
          <p
            role="status"
            className="flex items-start gap-1.5 text-chrome-micro text-chrome-menu-text"
          >
            <FolderGlyph className="mt-px size-3.5 shrink-0 text-chrome-muted" />
            <span className="min-w-0 break-words">{firstRunNotice}</span>
          </p>
        ) : null}
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
        {torrent ? (
          <p className="text-chrome-micro text-chrome-menu-text">
            This is a magnet link. It opens in Add torrent, where its files are
            chosen before anything starts.
          </p>
        ) : null}

        {valid ? (
          <div className="flex min-w-0 items-center gap-2.5">
            <FileTypeIcon
              category={category}
              icon={categoryIcon}
              name={name}
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

        {valid && probe.kind === "failed" && !videoPage ? (
          <p
            role="status"
            className="flex items-start gap-1.5 text-chrome-micro text-chrome-menu-text"
          >
            <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0 break-words">{probe.reason}</span>
          </p>
        ) : null}

        {valid && gallery?.isGallery ? (
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-chrome bg-chrome-canvas px-3 py-2.5">
            <p className="min-w-0 text-chrome-micro text-chrome-menu-text">
              This page looks like a gallery of{" "}
              <span className="tabular-nums">{gallery.count}</span> images.
              {gallery.truncated ? " Only the first images are offered." : null}
            </p>
            <DialogAction
              tone="quiet"
              disabled={busy}
              onClick={() => void downloadGallery()}
            >
              Download gallery
            </DialogAction>
          </div>
        ) : null}

        {/* A page that plays a video shows its qualities here, in the same
            list the quality panel draws: the reader chooses without a second
            window. The files below it are the ones the markup names plainly,
            and only when the page's video could not be read. */}
        {videoPage ? (
          <div className="rounded-chrome bg-chrome-canvas px-3 py-2.5">
            <Tooltip content={qualitySummary} side="top" className="block min-w-0">
              <p className="truncate text-chrome-micro text-chrome-menu-text">
                {qualitySummary}
              </p>
            </Tooltip>
            {qualityError ? (
              <p
                role="alert"
                className="mt-1 text-chrome-micro break-words text-chrome-error"
              >
                {qualityError}
              </p>
            ) : null}
            {qualities.length > 0 ? (
              <div className="mt-1.5 flex max-h-56 flex-col">
                <FormatList
                  rows={qualities}
                  value={qualityChoice}
                  onSelect={chooseQuality}
                  onActivate={() => void submit()}
                  disabled={busy}
                />
              </div>
            ) : null}
          </div>
        ) : null}

        {valid && media && fileRows.length > 0 && (!videoPage || qualityError) ? (
          <div className="rounded-chrome bg-chrome-canvas px-3 py-2.5">
            <p className="text-chrome-micro text-chrome-menu-text">
              {fileRows.length === 1
                ? "This page offers one media file."
                : `This page offers ${fileRows.length} media files.`}
              {media.truncated ? " Only the first ones are shown." : null}
            </p>
            <div className="mt-1.5 flex max-h-56 flex-col">
              <FormatCheckList
                label="Media files"
                rows={fileRows}
                values={mediaChoice}
                onToggle={toggleMedia}
                disabled={busy}
              />
            </div>
            <div className="mt-2 flex justify-end">
              <DialogAction
                tone="quiet"
                disabled={busy || mediaChoice.length === 0}
                onClick={() => void downloadMediaFiles()}
              >
                Download selected
              </DialogAction>
            </div>
          </div>
        ) : null}

        {valid && debridAvailable ? (
          <div className="flex items-center justify-between gap-3">
            <span className="text-chrome-micro text-chrome-muted">
              Unlock this link when a service can
            </span>
            <Switch
              label="Unlock this link when a service can"
              checked={debridUnlock}
              onChange={setDebridUnlock}
            />
          </div>
        ) : null}

        {valid && driveDocument ? (
          <div>
            <span className={LABEL}>Export as</span>
            <div className="w-52">
              <ListBox
                label="Export as"
                value={driveExport}
                onChange={setDriveExport}
                options={driveExports(driveDocument)}
              />
            </div>
          </div>
        ) : null}

        {valid && drive?.kind === "folder" ? (
          <p
            role="status"
            className="flex items-start gap-1.5 text-chrome-micro text-chrome-menu-text"
          >
            <WarningMark tone="amber" className="mt-px size-3.5 shrink-0" />
            <span className="min-w-0 break-words">
              This build cannot open a shared Drive folder yet. Pick a file
              link instead.
            </span>
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
