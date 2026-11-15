import { useId, useRef, useState } from "react";

import { open as pickFile } from "@tauri-apps/plugin-dialog";

import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { TorrentFileList } from "@/components/workspace/torrent-file-list";
import { formatSize } from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import {
  torrentLookup,
  torrentStart,
  type TorrentInfo,
} from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

interface AddTorrentDialogProps {
  open: boolean;
  onClose: () => void;
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
    return "Paste a magnet link, or open a .torrent file.";
  }
  const torrentish =
    address.toLowerCase().startsWith("magnet:") ||
    address.toLowerCase().endsWith(".torrent");
  return torrentish ? null : "That is not a magnet link or a .torrent file.";
}

/**
 * Add torrent: a magnet link or the `.torrent` file itself.
 *
 * The link is looked up first and its files are listed, the way the reference
 * download manager does it: a magnet asks its peers for the list, a file says
 * it at once. Only the ticked files are downloaded, into a folder named after
 * the torrent, and the torrent window then shows what the job is doing.
 */
export function AddTorrentDialog({ open, onClose }: AddTorrentDialogProps) {
  const fieldId = useId();
  const [source, setSource] = useState("");
  const [phase, setPhase] = useState<Phase>("link");
  const [info, setInfo] = useState<TorrentInfo | null>(null);
  const [selected, setSelected] = useState<number[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A look-up that was left behind still answers; only the newest one counts.
  const asked = useRef(0);

  function reset() {
    asked.current += 1;
    setSource("");
    setPhase("link");
    setInfo(null);
    setSelected([]);
    setProblem(null);
    setBusy(false);
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

  async function lookUp() {
    if (busy || phase === "looking") {
      return;
    }
    const trouble = torrentProblem(source);
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
    const generation = ++asked.current;
    try {
      const found = await torrentLookup(source.trim());
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

  /** Leave the wait behind and go back to the link. */
  function stopLooking() {
    asked.current += 1;
    setPhase("link");
  }

  async function start() {
    if (busy || !info || selected.length === 0) {
      return;
    }
    setBusy(true);
    setProblem(null);
    try {
      // The engine opens the torrent window on its own once the job is in.
      await torrentStart(source.trim(), undefined, selected);
      reset();
      onClose();
    } catch (failure) {
      setProblem(describeError(failure));
      setBusy(false);
    }
  }

  const footer =
    phase === "files" ? (
      <>
        <DialogAction
          tone="quiet"
          className="mr-auto"
          disabled={busy}
          onClick={() => {
            setPhase("link");
            setProblem(null);
          }}
        >
          Back
        </DialogAction>
        <DialogAction onClick={close} disabled={busy}>
          Cancel
        </DialogAction>
        <DialogAction
          tone="primary"
          disabled={busy || selected.length === 0}
          onClick={() => void start()}
        >
          Add torrent
        </DialogAction>
      </>
    ) : (
      <>
        <DialogAction onClick={phase === "looking" ? stopLooking : close}>
          Cancel
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
      className="w-[520px]"
      footer={footer}
    >
      {phase === "files" && info ? (
        <>
          <div className="flex items-baseline justify-between gap-3 pb-2">
            <p className="min-w-0 truncate text-xs text-chrome-title-text">
              {info.name}
            </p>
            <p className="shrink-0 text-chrome-micro text-chrome-muted">
              {formatSize(info.totalLength)}
            </p>
          </div>
          <TorrentFileList
            files={info.files}
            selected={selected}
            onChange={setSelected}
            disabled={busy}
          />
          <p className="pt-2 text-chrome-micro leading-4 text-chrome-muted">
            Saved in a folder named after the torrent.
          </p>
        </>
      ) : (
        <>
          <label
            htmlFor={fieldId}
            className="block pb-1 text-xs text-chrome-menu-text"
          >
            Magnet link
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
                ? "Looking for the torrent's files. A magnet needs a peer to answer; this can take a moment."
                : "Saved in a folder named after the torrent."}
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
