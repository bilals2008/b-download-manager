import { useEffect, useId, useRef, useState } from "react";

import { open as pickFile } from "@tauri-apps/plugin-dialog";

import { DownloadGlyph, SpinnerGlyph } from "@/components/icons/ui-icons";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { Tooltip } from "@/components/ui/tooltip";
import {
  FolderGlyph,
  TorrentFileTree,
} from "@/components/workspace/torrent-file-tree";
import { formatSize } from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import { chooseFolder } from "@/lib/ipc";
import {
  torrentLookup,
  torrentStart,
  type TorrentInfo,
  type TorrentStartOptions,
} from "@/lib/ipc-torrent";
import { preferredFolder } from "@/lib/settings";

interface AddTorrentDialogProps {
  open: boolean;
  onClose: () => void;
  /** A magnet or a `.torrent` handed over from Add URL. It is looked up as
   * the dialog opens, so the files show without a second press. */
  initialSource?: string | null;
}

/**
 * Where the dialog is: the link, the wait for its file list, the files, or a
 * look-up that did not answer.
 *
 * The wait is never a gate. A magnet is a job the engine can start on its own;
 * the list only lets the reader leave files out, so a swarm that is slow to
 * answer must not keep the torrent out of the app.
 */
type Phase = "link" | "looking" | "files" | "unanswered";

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

/** Whether the link is a magnet, which the engine can start without a list. */
function isMagnet(source: string): boolean {
  return source.trim().toLowerCase().startsWith("magnet:");
}

/** The name the magnet carries in its `dn`, before any peer has answered. */
function magnetName(link: string): string | null {
  const at = link.indexOf("?");
  if (at < 0) {
    return null;
  }
  for (const pair of link.slice(at + 1).split("&")) {
    const [key, value = ""] = pair.split("=");
    if (key !== "dn") {
      continue;
    }
    try {
      const name = decodeURIComponent(value.replace(/\+/g, " ")).trim();
      return name || null;
    } catch {
      return value.trim() || null;
    }
  }
  return null;
}

const FOLDER_ROW =
  "group flex h-9 w-full min-w-0 items-center gap-2 rounded-chrome bg-chrome-canvas px-2.5 text-left transition-colors hover:bg-chrome-popup-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-60";

/**
 * Add torrent: a magnet link, the `.torrent` file itself, or its address.
 *
 * The link is read first and its files are listed, so the reader can leave out
 * what they do not want. A magnet that no peer answers for is still a job: the
 * dialog says the list is not in yet and offers to add the torrent anyway,
 * because waiting for the swarm is not a reason to refuse the link.
 */
export function AddTorrentDialog({
  open,
  onClose,
  initialSource,
}: AddTorrentDialogProps) {
  const fieldId = useId();
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
    const timer = window.setInterval(
      () => setWaited((seconds) => seconds + 1),
      1000,
    );
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
      void lookUp(picked);
    }
  }

  async function pickFolder() {
    if (busy) {
      return;
    }
    try {
      const picked = await chooseFolder(folder?.trim() || preferredFolder());
      if (picked) {
        setFolder(picked);
      }
    } catch (failure) {
      setProblem(describeError(failure));
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
      setPhase("unanswered");
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

  async function start(options: TorrentStartOptions, blind = false) {
    if (busy) {
      return;
    }
    if (!blind && (!info || selected.length === 0)) {
      return;
    }
    const value = source.trim();
    if (!value) {
      return;
    }
    setBusy(true);
    setProblem(null);
    try {
      // The engine opens the torrent window on its own once the job is in.
      await torrentStart(
        value,
        folder ?? undefined,
        blind ? undefined : selected,
        options,
      );
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
  const magnet = isMagnet(source);
  const carried = magnet ? magnetName(source) : null;
  const title =
    info?.name ?? carried ?? (source ? source.replace(/\.[^.\\/]+$/, "") : "");
  const filesHint = saveFolder
    ? `Files go to ${saveFolder}${info ? ` in a folder named ${info.name}` : ""}.`
    : "Files go to your download folder, in a folder named after the torrent.";

  const footer =
    phase === "files" && info ? (
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
          icon={<DownloadGlyph className="size-3.5" />}
          disabled={busy || selected.length === 0}
          onClick={() => void start({ sequential: inOrder })}
        >
          {busy ? "Starting…" : "Download now"}
        </DialogAction>
        <DialogAction
          disabled={busy || selected.length === 0}
          onClick={() => void start({ startPaused: true, sequential: inOrder })}
        >
          Later
        </DialogAction>
        <DialogAction onClick={close} disabled={busy}>
          Cancel
        </DialogAction>
      </>
    ) : phase === "looking" ? (
      <>
        <DialogAction onClick={stopLooking}>Stop</DialogAction>
        {magnet ? (
          <DialogAction
            tone="primary"
            disabled={busy}
            onClick={() => void start({ sequential: inOrder }, true)}
          >
            Add without the list
          </DialogAction>
        ) : null}
        <DialogAction onClick={close} disabled={busy}>
          Cancel
        </DialogAction>
      </>
    ) : phase === "unanswered" ? (
      <>
        <DialogAction onClick={() => void lookUp()}>Try again</DialogAction>
        {magnet ? (
          <DialogAction
            tone="primary"
            disabled={busy}
            onClick={() => void start({ sequential: inOrder }, true)}
          >
            Add anyway
          </DialogAction>
        ) : null}
        <DialogAction onClick={close} disabled={busy}>
          Cancel
        </DialogAction>
      </>
    ) : (
      <>
        <DialogAction
          tone="quiet"
          className="mr-auto"
          disabled={busy}
          onClick={() => void pickTorrentFile()}
        >
          Open .torrent file…
        </DialogAction>
        <DialogAction onClick={close} disabled={busy}>
          Cancel
        </DialogAction>
        <DialogAction
          tone="primary"
          disabled={busy || !source.trim()}
          onClick={() => void lookUp()}
        >
          Look up
        </DialogAction>
      </>
    );

  return (
    <Dialog
      open={open}
      title="Add torrent"
      onClose={close}
      dismissible={!busy}
      className="w-[min(38rem,calc(100vw-2rem))]"
      footer={footer}
    >
      {phase === "files" && info ? (
        <div className="flex flex-col gap-3">
          <div className="min-w-0">
            <Tooltip content={info.name} side="top" className="block min-w-0">
              <p className="truncate text-[13px] font-medium text-chrome-title-text">
                {info.name}
              </p>
            </Tooltip>
            <p className="mt-0.5 text-chrome-micro tabular-nums text-chrome-muted">
              {info.files.length}{" "}
              {info.files.length === 1 ? "file" : "files"} ·{" "}
              {formatSize(info.totalLength)} ·{" "}
              {info.trackers.length === 0
                ? "no trackers, peers from the peer table"
                : `${info.trackers.length} ${info.trackers.length === 1 ? "tracker" : "trackers"}`}
            </p>
          </div>

          <TorrentFileTree
            files={info.files}
            selected={selected}
            onChange={setSelected}
            disabled={busy}
          />

          <div>
            <span className="mb-1.5 block text-xs text-chrome-muted">
              Save to
            </span>
            <Tooltip
              content={saveFolder || "Your download folder"}
              side="top"
              className="block w-full min-w-0"
            >
              <button
                type="button"
                onClick={() => void pickFolder()}
                disabled={busy}
                aria-label={
                  saveFolder
                    ? `Change the save folder, currently ${saveFolder}`
                    : "Choose a save folder"
                }
                className={FOLDER_ROW}
              >
                <FolderGlyph className="size-3.5 shrink-0 text-chrome-muted" />
                <span className="min-w-0 flex-1 truncate font-mono text-chrome-micro text-chrome-menu-text">
                  {saveFolder || "Your download folder"}
                </span>
                <span className="shrink-0 text-chrome-micro text-chrome-muted transition-colors group-hover:text-chrome-menu-text">
                  Change
                </span>
              </button>
            </Tooltip>
            <p id={filesHintId} className="mt-1.5 text-chrome-micro text-chrome-muted">
              {filesHint}
            </p>
          </div>

          {problem ? (
            <p role="alert" className="text-chrome-micro break-words text-chrome-error">
              {problem}
            </p>
          ) : null}
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <div>
            <label
              htmlFor={fieldId}
              className="mb-1.5 block text-xs text-chrome-muted"
            >
              Magnet link, .torrent file or address
            </label>
            <input
              id={fieldId}
              data-autofocus
              value={source}
              placeholder="magnet:?xt=urn:btih:…"
              spellCheck={false}
              autoComplete="off"
              readOnly={phase === "looking"}
              aria-invalid={problem !== null || undefined}
              aria-describedby={problem ? `${fieldId}-problem` : filesHintId}
              className={`${FIELD} h-9 font-mono`}
              onChange={(event) => {
                setSource(event.target.value);
                setProblem(null);
                if (phase === "unanswered") {
                  setPhase("link");
                }
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void lookUp();
                }
              }}
            />
          </div>

          {phase === "looking" ? (
            <div className="rounded-chrome bg-chrome-canvas px-3 py-2.5">
              <p className="flex items-center gap-2 text-xs text-chrome-menu-text">
                <SpinnerGlyph className="size-3.5 shrink-0 text-chrome-muted motion-safe:animate-spin" />
                <span className="min-w-0 truncate">
                  {title
                    ? `Looking for the files of ${title}`
                    : "Looking for the torrent's files"}
                </span>
                <span className="ml-auto shrink-0 text-chrome-micro tabular-nums text-chrome-muted">
                  {waited}s
                </span>
              </p>
              <p className="mt-1 text-chrome-micro text-chrome-muted">
                {magnet
                  ? "A magnet asks a peer for its file list and some swarms take a while to answer. The torrent can be added now and the list appears in its window once a peer answers."
                  : "The address is being read; a .torrent file usually answers at once."}
              </p>
            </div>
          ) : (
            <p id={filesHintId} className="text-chrome-micro text-chrome-muted">
              {source.trim()
                ? filesHint
                : "A magnet needs a peer to name its files. A .torrent file already holds them."}
            </p>
          )}

          {phase === "unanswered" && magnet ? (
            <div className="rounded-chrome bg-chrome-canvas px-3 py-2.5">
              <p className="text-xs text-chrome-menu-text">
                No peer sent the file list yet.
              </p>
              <p className="mt-1 text-chrome-micro text-chrome-muted">
                The torrent can still be added. Its window shows the files as
                soon as the swarm answers, and the download starts from them.
              </p>
            </div>
          ) : null}

          {magnet && phase !== "looking" ? (
            <p className="text-chrome-micro text-chrome-muted">
              The magnet names {carried || "this torrent"}; its size is known
              once a peer answers.
            </p>
          ) : null}

          {problem ? (
            <p
              id={`${fieldId}-problem`}
              role="alert"
              className="text-chrome-micro break-words text-chrome-error"
            >
              {problem}
            </p>
          ) : null}
        </div>
      )}
    </Dialog>
  );
}
