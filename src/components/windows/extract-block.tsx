import { useEffect, useState } from "react";

import { FIELD } from "@/components/ui/field";
import {
  extractDownloadWithPassword,
  extractPending,
  onExtractFailed,
  onExtractNeedsPassword,
  onExtractOutcome,
  onExtractProgress,
} from "@/lib/ipc-extract";
import { describeError, isDesktop } from "@/lib/errors";

/** What the archive is doing, and what the reader can do about it. */
type ExtractState =
  | { kind: "idle" }
  | { kind: "packing"; files: number }
  | { kind: "password"; name: string; attempts: number; wrong: boolean }
  | { kind: "done"; files: number; renamed: number; skipped: number; refused: number }
  | { kind: "failed"; message: string };

/**
 * The archive that came down with this file.
 *
 * The engine unpacks it beside itself when the setting asks for it. This
 * block shows how far it got, asks for the password of an encrypted archive,
 * and says what came out — or why it could not be.
 *
 * One password attempt is given, then one more; after that the reader is sent
 * to the archive itself rather than five attempts at a locked box. The
 * password is never written anywhere: it is used for the one unpack it was
 * typed for and then dropped.
 */
export function ExtractBlock({ id }: { id: number }) {
  const [state, setState] = useState<ExtractState>({ kind: "idle" });
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    const stops: Array<() => void> = [];

    const keep = (listen: () => Promise<() => void>) =>
      listen()
        .then((stop) => {
          if (active) {
            stops.push(stop);
          } else {
            stop();
          }
        })
        .catch(() => {
          // No desktop runtime, no block.
        });
    void extractPending()
      .then((pending) => {
        if (active && pending && pending.id === id) {
          setState({ kind: "password", name: pending.name, attempts: 0, wrong: false });
        }
      })
      .catch(() => {
        // Asking is a nicety; the event is the source of truth.
      });
    void keep(() =>
        onExtractProgress((event) => {
          if (event.id === id) {
            setState({ kind: "packing", files: event.files });
          }
        }),
    );
    void keep(() =>
        onExtractNeedsPassword((event) => {
          if (event.id === id) {
            setState({ kind: "password", name: event.name, attempts: 0, wrong: false });
          }
        }),
    );
    void keep(() =>
        onExtractOutcome((event) => {
          if (event.id === id) {
            setState({
              kind: "done",
              files: event.files,
              renamed: event.renamed,
              skipped: event.skipped,
              refused: event.refused,
            });
          }
        }),
    );
    void keep(() =>
        onExtractFailed((event) => {
          if (event.id === id) {
            setState({ kind: "failed", message: event.message });
          }
        }),
    );
    return () => {
      active = false;
      for (const stop of stops) {
        stop();
      }
    };
    // The listeners only care about the id.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  async function unpack() {
    if (busy || state.kind !== "password") {
      return;
    }
    setBusy(true);
    try {
      await extractDownloadWithPassword(id, password);
      setState({ kind: "done", files: 0, renamed: 0, skipped: 0, refused: 0 });
      setPassword("");
    } catch (failure) {
      const message = describeError(failure);
      const wrong = /password|encrypted/i.test(message);
      setState(
        wrong
          ? { ...state, attempts: state.attempts + 1, wrong: true }
          : { kind: "failed", message },
      );
      setPassword("");
    } finally {
      setBusy(false);
    }
  }

  if (state.kind === "idle") {
    return null;
  }

  if (state.kind === "packing") {
    return (
      <p className="pt-1 text-chrome-micro text-chrome-muted" role="status">
        Unpacking. ({state.files} files so far)
      </p>
    );
  }

  if (state.kind === "done") {
    const notes: string[] = [];
    if (state.renamed > 0) {
      notes.push(`renamed ${state.renamed} that were already there`);
    }
    if (state.skipped > 0) {
      notes.push(`skipped ${state.skipped}`);
    }
    return (
      <div className="pt-1 text-chrome-micro text-chrome-muted" role="status">
        <p>
          Unpacked {state.files} files{notes.length > 0 ? ` — ${notes.join(", ")}` : ""}.
        </p>
        {state.refused > 0 ? (
          <p className="text-chrome-error">
            {state.refused} entries were refused; the archive is still there.
          </p>
        ) : null}
      </div>
    );
  }

  if (state.kind === "failed") {
    return (
      <div className="pt-1 text-chrome-micro text-chrome-error" role="alert">
        <p>The archive could not be unpacked.</p>
        <p>{state.message}</p>
      </div>
    );
  }

  // The password.
  const exhausted = state.attempts >= 2;
  return (
    <div className="flex flex-col gap-1 pt-1">
      <label htmlFor={`archive-password-${id}`} className="text-chrome-micro text-chrome-muted">
        {state.wrong ? "Wrong password." : `${state.name} needs a password.`}
      </label>
      <div className="flex items-center gap-2">
        <input
          id={`archive-password-${id}`}
          type="password"
          autoComplete="off"
          value={password}
          disabled={exhausted}
          onChange={(event) => setPassword(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              void unpack();
            }
          }}
          className={FIELD}
          placeholder="Password"
        />
        <button
          type="button"
          onClick={() => void unpack()}
          disabled={busy || exhausted || password.trim() === ""}
          className="h-7 shrink-0 rounded-chrome bg-chrome-menu-hover px-3 text-xs text-chrome-menu-text transition-colors hover:bg-chrome-menu-active disabled:opacity-45"
        >
          Unpack
        </button>
      </div>
      {exhausted ? (
        <p className="text-chrome-micro text-chrome-error">
          Still wrong. Open the archive another way.
        </p>
      ) : null}
    </div>
  );
}
