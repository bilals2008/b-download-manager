import { useState } from "react";

import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { Tooltip } from "@/components/ui/tooltip";
import { describeError } from "@/lib/errors";
import {
  torrentCheckTrackers,
  type TrackerCheck,
} from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

interface AddTrackersDialogProps {
  /** Whether the dialog is on screen. */
  open: boolean;
  /** Close it; the state is dropped, so the next open starts clean. */
  onClose: () => void;
  /** The job the trackers are added to. */
  jobId: string;
  /** Add the addresses and read the job again. */
  onAdd: (urls: string[]) => Promise<void>;
}

/** Where the dialog is: the addresses, the check, or what the check found. */
type Phase = "edit" | "checking" | "results";

/** Split a pasted list into one address per entry, without repeats. */
function addressesOf(text: string): string[] {
  const seen = new Set<string>();
  const urls: string[] = [];
  for (const piece of text.split(/[\s,]+/)) {
    let url = piece.trim();
    if (!url) {
      continue;
    }
    // A tracker list is usually pasted without the scheme; udp is what most
    // of them use, and a scheme-less line is not an address otherwise.
    if (!url.includes("://")) {
      url = `udp://${url}`;
    }
    const key = url.toLowerCase();
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    urls.push(url);
  }
  return urls;
}

/** What is wrong with one address, or null when it is a tracker address. */
function addressProblem(url: string): string | null {
  if (!/^(https?|udp):\/\/[^\s/]+/i.test(url)) {
    return `“${url}” is not a tracker address. It starts with http://, https:// or udp://.`;
  }
  return null;
}

/** The host of a tracker, without the scheme or the path. */
function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url.replace(/^[a-z+]+:\/\//i, "").split("/")[0] || url;
  }
}

/**
 * Add trackers to a torrent, the way a torrent client does.
 *
 * The addresses are checked first, unless the reader turns that off: a tracker
 * that cannot be reached is a dead line in a list, and the reader is better
 * served by seeing "no answer" before it is saved than by a tracker that never
 * answers afterwards. Only the ones that answered are added; one that failed
 * can still be added with "Add anyway".
 */
export function AddTrackersDialog({
  open,
  onClose,
  jobId,
  onAdd,
}: AddTrackersDialogProps) {
  const [text, setText] = useState("");
  const [check, setCheck] = useState(true);
  const [phase, setPhase] = useState<Phase>("edit");
  const [checks, setChecks] = useState<TrackerCheck[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const urls = addressesOf(text);
  const answered = checks.filter((entry) => entry.ok);

  function close() {
    if (busy) {
      return;
    }
    setText("");
    setCheck(true);
    setPhase("edit");
    setChecks([]);
    setError(null);
    onClose();
  }

  async function save(list: string[]) {
    setBusy(true);
    setError(null);
    try {
      await onAdd(list);
      setBusy(false);
      close();
    } catch (failure) {
      setBusy(false);
      setError(describeError(failure));
    }
  }

  async function submit() {
    if (urls.length === 0) {
      setError("Add at least one tracker address.");
      return;
    }
    for (const url of urls) {
      const problem = addressProblem(url);
      if (problem) {
        setError(problem);
        return;
      }
    }
    if (!check) {
      await save(urls);
      return;
    }
    setPhase("checking");
    setError(null);
    try {
      const found = await torrentCheckTrackers(jobId, urls);
      setChecks(found);
      setPhase("results");
    } catch (failure) {
      setPhase("edit");
      setError(describeError(failure));
    }
  }

  const footer =
    phase === "results" ? (
      <>
        <DialogAction tone="quiet" disabled={busy} onClick={close}>
          Cancel
        </DialogAction>
        <DialogAction
          tone="primary"
          disabled={busy || answered.length === 0}
          onClick={() => void save(answered.map((entry) => entry.url))}
        >
          {busy
            ? "Adding…"
            : `Add ${answered.length} ${answered.length === 1 ? "tracker" : "trackers"}`}
        </DialogAction>
      </>
    ) : (
      <>
        <DialogAction
          tone="quiet"
          disabled={busy || phase === "checking"}
          onClick={close}
        >
          Cancel
        </DialogAction>
        <DialogAction
          tone="primary"
          disabled={busy || phase === "checking" || urls.length === 0}
          onClick={() => void submit()}
        >
          {phase === "checking"
            ? "Checking…"
            : check
              ? `Check ${urls.length === 1 ? "tracker" : `${urls.length} trackers`}`
              : "Add trackers"}
        </DialogAction>
      </>
    );

  return (
    <Dialog
      open={open}
      title="Add trackers"
      onClose={close}
      dismissible={!busy}
      footer={footer}
      className="max-w-lg"
    >
      {phase === "results" ? (
        <div className="flex flex-col gap-3">
          <p className="text-xs text-chrome-menu-text">
            {answered.length} of {checks.length}{" "}
            {checks.length === 1 ? "tracker answered" : "trackers answered"}.
            Only the ones that answered will be added.
          </p>
          <ul className="flex max-h-60 flex-col gap-0.5 overflow-y-auto scrollbar-thin">
            {checks.map((entry) => (
              <li
                key={entry.url}
                className="flex items-baseline justify-between gap-3 py-1 text-xs"
              >
                <Tooltip content={entry.url} className="min-w-0 flex-1">
                  <span className="block truncate text-chrome-menu-text">
                    {hostOf(entry.url)}
                  </span>
                </Tooltip>
                <Tooltip
                  content={
                    entry.ok
                      ? "The tracker answered with a peer list."
                      : entry.message || "The tracker did not answer."
                  }
                  className="w-44 justify-end"
                  side="left"
                >
                  <span
                    className={cn(
                      "block truncate text-right text-chrome-micro",
                      entry.ok ? "text-chrome-muted" : "text-chrome-error",
                    )}
                  >
                    {entry.ok
                      ? entry.peers === 1
                        ? "1 peer"
                        : `${entry.peers} peers`
                      : "No answer"}
                  </span>
                </Tooltip>
              </li>
            ))}
          </ul>
          {answered.length < checks.length ? (
            <button
              type="button"
              disabled={busy}
              onClick={() => void save(checks.map((entry) => entry.url))}
              className="self-start text-chrome-micro text-chrome-muted underline-offset-2 hover:text-chrome-menu-text hover:underline focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none disabled:opacity-60"
            >
              Add all of them anyway
            </button>
          ) : null}
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1.5">
            <span className="text-chrome-micro text-chrome-muted">
              Tracker addresses, one per line
            </span>
            <textarea
              value={text}
              onChange={(event) => {
                setText(event.target.value);
                setError(null);
              }}
              rows={6}
              spellCheck={false}
              placeholder={"udp://tracker.example.org:6969/announce\nhttps://tracker.example.net/announce"}
              className={cn(FIELD, "resize-none font-mono text-chrome-micro")}
            />
          </label>
          <div className="flex items-center gap-2.5">
            <Switch
              checked={check}
              onChange={setCheck}
              label="Check each tracker before adding it"
            />
            <button
              type="button"
              onClick={() => setCheck(!check)}
              className="text-left text-xs text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none"
            >
              Check each one first
            </button>
          </div>
          <p className="text-chrome-micro leading-4 text-chrome-muted">
            A tracker is a server that says who else has this torrent. If the
            torrent is stuck at zero, a tracker list from the torrent&rsquo;s
            page is what it needs.
          </p>
        </div>
      )}
      {error ? (
        <p role="alert" className="mt-3 text-chrome-micro text-chrome-error">
          {error}
        </p>
      ) : null}
    </Dialog>
  );
}
