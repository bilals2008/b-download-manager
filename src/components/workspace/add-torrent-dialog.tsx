import { useEffect, useId, useRef, useState } from "react";

import { open as pickFile } from "@tauri-apps/plugin-dialog";

import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { Checkbox } from "@/components/ui/checkbox";
import { FIELD } from "@/components/ui/field";
import {
  FolderGlyph,
  TorrentFileTree,
} from "@/components/workspace/torrent-file-tree";
import { formatSize, joinPath } from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import {
  torrentLookup,
  torrentStart,
  type TorrentInfo,
  type TorrentStartOptions,
} from "@/lib/ipc-torrent";
import { preferredFolder } from "@/lib/settings";
import { cn } from "@/lib/utils";

interface AddTorrentDialogProps {
  open: boolean;
  onClose: () => void;
  /** A magnet or a `.torrent` handed over from Add URL. It is looked up as
   * the dialog opens, so the files show without a second press. */
  initialSource?: string | null;
}

/**
 * Where the dialog is: the link, the wait for its file list, or the files.
 * A torrent is read before it starts, so the reader sees what it holds and
 * leaves out what they do not want.
 */
type Phase = "link" | "looking" | "files";

/** What the field still needs, or null when it can be looked up. */
function torrentProblem(source: string): string | null {
  const address = source.trim();
  if (!address) {
    return "Paste a magnet link, open a .torrent file, or paste its address.";
  }
  const lower = address.toLowerCase();
  const torrentish =
    lower.startsWith("magnet:") ||
    lower.endsWith(".torrent") ||
    lower.startsWith("http://") ||
    lower.startsWith("https://");
  return torrentish ? null : "That is not a magnet link, a .torrent file or an address.";
}

/** The small browse button that sits at a field's right edge. */
function EllipsisButton({
  label,
  onClick,
  disabled,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
      className="flex w-8 shrink-0 items-center justify-center rounded-r-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-45 disabled:hover:bg-transparent disabled:hover:text-chrome-muted"
    >
      <span aria-hidden="true">...</span>
    </button>
  );
}

/**
 * Add torrent: a magnet link, the `.torrent` file itself, or its address.
 *
 * The link is looked up first and its files are listed, the way the reference
 * download manager does it: a magnet asks its peers for the list, a file says
 * it at once. Only the ticked files are downloaded, into a folder named after
 * the torrent, and the torrent window then shows what the job is doing.
 */
export function AddTorrentDialog({
  open,
  onClose,
  initialSource,
}: AddTorrentDialogProps) {
  const fieldId = useId();
  const saveFieldId = `${fieldId}-save`;
  const filesHintId = `${fieldId}-files`;
  const [source, setSource] = useState("");
  const [phase, setPhase] = useState<Phase>("link");
  const [info, setInfo] = useState<TorrentInfo | null>(null);
  const [selected, setSelected] = useState<number[]>([]);
  const [folder, setFolder] = useState<string | null>(null);
  const [inOrder, setInOrder] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A look-up that was left behind still answers; only the newest one counts.
  const asked = useRef(0);
  // The handed-over link that was already looked up, so one open is one
  // look-up even though the effect runs again on every render.
  const seeded = useRef<string | null>(null);
  // How long the look-up has been waiting, so the dialog is not silent.
  const [waited, setWaited] = useState(0);

  useEffect(() => {
    if (phase !== "looking") {
      return;
    }
    const timer = window.setInterval(() => setWaited((seconds) => seconds + 1), 1000);
    return () => window.clearInterval(timer);
  }, [phase]);

  function reset() {
    asked.current += 1;
    seeded.current = null;
    setSource("");
    setPhase("link");
    setInfo(null);
    setSelected([]);
    setFolder(null);
    setInOrder(false);
    setProblem(null);
    setBusy(false);
    setWaited(0);
  }

  function close() {
    if (busy) {
      return;
    }
    reset();
    onClose();
  }

  async function pickTorrentFile() {
    if (busy) {
      return;
    }
    const picked = await pickFile({
      multiple: false,
      directory: false,
      filters: [{ name: "Torrent", extensions: ["torrent"] }],
    });
    if (typeof picked === "string" && picked) {
      setSource(picked);
      setProblem(null);
    }
  }

  async function pickFolder() {
    if (busy) {
      return;
    }
    const picked = await pickFile({ directory: true, multiple: false });
    if (typeof picked === "string" && picked) {
      setFolder(picked);
    }
  }

  async function lookUp(given?: string) {
    if (busy || phase === "looking") {
      return;
    }
    // A handed-over link is passed in: the state has not caught up with it yet
    // on the render that starts the look-up.
    const value = given ?? source;
    const trouble = torrentProblem(value);
    if (trouble) {
      setProblem(trouble);
      return;
    }
    if (!isDesktop()) {
      setProblem("Torrents work only in the desktop app.");
      return;
    }
    setProblem(null);
    setPhase("looking");
    setWaited(0);
    const generation = ++asked.current;
    try {
      const found = await torrentLookup(value.trim());
      if (generation !== asked.current) {
        return;
      }
      setInfo(found);
      setSelected(found.files.map((_, index) => index));
      setPhase("files");
    } catch (failure) {
      if (generation !== asked.current) {
        return;
      }
      setProblem(describeError(failure));
      setPhase("link");
    }
  }

  // A link handed over from Add URL is looked up as the dialog opens: the
  // reader already chose a torrent, so there is nothing to press first.
  useEffect(() => {
    if (!open || !initialSource || seeded.current === initialSource) {
      return;
    }
    seeded.current = initialSource;
    setSource(initialSource);
    setProblem(null);
    void lookUp(initialSource);
  }, [open, initialSource]);

  /** Leave the wait behind and go back to the link. */
  function stopLooking() {
    asked.current += 1;
    setPhase("link");
    setWaited(0);
  }

  async function start(options: TorrentStartOptions) {
    if (busy || !info || selected.length === 0) {
      return;
    }
    setBusy(true);
    setProblem(null);
    try {
      // The engine opens the torrent window on its own once the job is in.
      await torrentStart(source.trim(), folder ?? undefined, selected, options);
      reset();
      onClose();
    } catch (failure) {
      setProblem(describeError(failure));
      setBusy(false);
    }
  }

  // The path the field and the hint show: the one the reader picked, then the
  // app's remembered or configured folder. An empty one means the engine's own
  // download folder, which has no honest path to print here.
  const saveFolder = folder?.trim() || preferredFolder().trim();
  const filesHint = saveFolder
    ? `Files go to: ${joinPath(saveFolder, info?.name ?? "")}`
    : "Saved in your download folder, in a folder named after the torrent.";

  const footer =
    phase === "files" ? (
      <>
        <Checkbox
          label="Download in order"
          checked={inOrder}
          disabled={busy}
          onChange={(event) => setInOrder(event.target.checked)}
          className="mr-auto"
        />
        <DialogAction
          tone="primary"
          disabled={busy || selected.length === 0}
          onClick={() => void start({ sequential: inOrder })}
        >
          Download Now
        </DialogAction>
        <DialogAction
          disabled={busy || selected.length === 0}
          onClick={() =>
            void start({ startPaused: true, sequential: inOrder })
          }
        >
          Later
        </DialogAction>
        <DialogAction onClick={close} disabled={busy}>
          Cancel
        </DialogAction>
      </>
    ) : (
      <>
        <DialogAction onClick={phase === "looking" ? stopLooking : close}>
          {phase === "looking" ? "Stop" : "Cancel"}
        </DialogAction>
        {phase === "looking" ? null : (
          <DialogAction tone="primary" onClick={() => void lookUp()}>
            Look up
          </DialogAction>
        )}
      </>
    );

  return (
    <Dialog
      open={open}
      title="Add Torrent"
      onClose={close}
      dismissible={!busy}
      className="w-[560px]"
      footer={footer}
    >
      {phase === "files" && info ? (
        <>
          <div className="flex items-center gap-2 pb-2">
            <FolderGlyph className="size-4 shrink-0 text-chrome-muted" />
            <p className="min-w-0 truncate text-sm font-medium text-chrome-title-text">
              {info.name} ({info.files.length}{" "}
              {info.files.length === 1 ? "file" : "files"}, {" "}
              {formatSize(info.totalLength)})
            </p>
          </div>
          <TorrentFileTree
            files={info.files}
            selected={selected}
            onChange={setSelected}
            disabled={busy}
          />
          <div className="pt-3">
            <label
              htmlFor={saveFieldId}
              className="block pb-1 text-xs text-chrome-menu-text"
            >
              Save to
            </label>
            <div className="relative">
              <input
                id={saveFieldId}
                readOnly
                value={saveFolder}
                placeholder="Your download folder"
                aria-describedby={filesHintId}
                className={cn(FIELD, "pr-9")}
              />
              <div className="absolute inset-y-0 right-0 flex">
                <EllipsisButton
                  label="Choose save folder"
                  disabled={busy}
                  onClick={() => void pickFolder()}
                />
              </div>
            </div>
            <p
              id={filesHintId}
              className="pt-1 text-chrome-micro text-chrome-muted"
            >
              {filesHint}
            </p>
          </div>
        </>
      ) : (
        <>
          <label
            htmlFor={fieldId}
            className="block pb-1 text-xs text-chrome-menu-text"
          >
            Magnet link, .torrent file or address
          </label>
          <input
            id={fieldId}
            data-autofocus
            value={source}
            placeholder="magnet:?xt=urn:btih:..."
            spellCheck={false}
            autoComplete="off"
            readOnly={phase === "looking"}
            aria-invalid={problem !== null || undefined}
            className={cn(
              FIELD,
              "font-mono",
              problem && "border-chrome-danger hover:border-chrome-danger",
            )}
            onChange={(event) => {
              setSource(event.target.value);
              setProblem(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                void lookUp();
              }
            }}
          />

          <div className="flex flex-wrap items-center gap-x-3 gap-y-2 pt-3">
            <DialogAction
              tone="quiet"
              disabled={phase === "looking"}
              onClick={() => void pickTorrentFile()}
            >
              Open .torrent file...
            </DialogAction>
            <p className="min-w-0 flex-1 text-chrome-micro leading-4 text-chrome-muted">
              {phase === "looking"
                ? `Looking for the torrent's files (${waited}s). A magnet needs a peer to answer; this can take a moment.`
                : "The files are looked up first, so you can leave out what you do not want."}
            </p>
          </div>
        </>
      )}

      {problem ? (
        <p role="alert" className="pt-2 text-chrome-micro text-chrome-error">
          {problem}
        </p>
      ) : null}
    </Dialog>
  );
}
