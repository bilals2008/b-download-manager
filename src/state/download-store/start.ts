import { useRef, useState, type Dispatch, type SetStateAction } from "react";

import {
  StartCancelled,
  startResolving,
  type BatchAsking,
  type ConflictAnswer,
  type ConflictReply,
  type FileConflict,
} from "@/lib/conflict";
import { joinPath, splitPath, type Download } from "@/lib/downloads";
import {
  redownloadDownload,
  startDownload,
  type BridgeDownload,
  type StartDownloadInput,
  type StartOutcome,
} from "@/lib/ipc";
import { loadSettings } from "@/lib/settings";

import type {
  AddOutcome,
  BrowserRequest,
  NewDownload,
  WorkspaceState,
} from "./types";

/** The pieces of the store the start path reaches for. */
interface StartDeps {
  setState: Dispatch<SetStateAction<WorkspaceState>>;
  forget: (id: number) => void;
}

/**
 * Starting a download: the destination question, a restart and a link the
 * browser handed over, before the row is in the list.
 */
export function useStartPath({ setState, forget }: StartDeps) {
  // A destination that is taken, while the File Exists dialog asks about it.
  const [conflict, setConflict] = useState<FileConflict | null>(null);
  const conflictReply = useRef<((reply: ConflictReply | null) => void) | null>(
    null,
  );
  // A link the browser extension handed over, waiting in the add dialog.
  const [browserRequest, setBrowserRequest] = useState<BrowserRequest | null>(
    null,
  );

  /** Put the File Exists question on screen and wait for the answer. */
  function askConflict(question: FileConflict): Promise<ConflictReply | null> {
    return new Promise((resolve) => {
      // A question that is still waiting can only be replaced by the next one;
      // settle it as dismissed so its start does not hang on a promise nobody
      // will ever answer.
      conflictReply.current?.(null);
      conflictReply.current = resolve;
      setConflict(question);
    });
  }

  /** Take the answer out of the ref first, so a late click cannot land twice. */
  function settleConflict(reply: ConflictReply | null) {
    const settle = conflictReply.current;
    conflictReply.current = null;
    setConflict(null);
    settle?.(reply);
  }

  /** Start a finished or failed download over with a fresh engine job. */
  async function restart(
    item: Download,
    batch?: BatchAsking,
  ): Promise<AddOutcome> {
    let outcome: StartOutcome;
    try {
      // The engine replaces the row itself once the destination question is
      // settled, so a cancelled question leaves the row untouched.
      outcome = await startResolving(
        (choice) => redownloadDownload(item.id, choice),
        askConflict,
        batch,
      );
    } catch (failure) {
      if (failure instanceof StartCancelled) {
        return "cancelled";
      }
      throw failure;
    }
    if (outcome.id === null || outcome.skipped) {
      return "skipped";
    }
    // The row was replaced in the engine: the old id is gone, and the update
    // of the new one fills the list.
    forget(item.id);
    return "started";
  }

  /** Start one download and put its row in the list. */
  async function beginDownload(input: NewDownload): Promise<AddOutcome> {
    const dest = joinPath(input.savePath, input.fileName);
    // The options window owns both values, and they can change while the add
    // dialog is open, so they are read at the moment the download starts.
    const settings = loadSettings();
    const connections = settings.defaultConnections;
    const policy = settings.duplicatePolicy;
    const request: StartDownloadInput = {
      url: input.url,
      dest,
      connections,
      referer: input.referer,
      queue: input.queue,
      category: input.category,
      description: input.description,
      driveExport: input.driveExport,
    };
    let outcome: StartOutcome;
    try {
      outcome = await startResolving(
        (choice) => startDownload({ ...request, conflict: choice }),
        askConflict,
        // A chosen answer is the first attempt; a refusal that it cannot
        // settle still becomes the File Exists question.
        policy === "ask"
          ? undefined
          : { remaining: 0, choice: policy as ConflictAnswer },
      );
    } catch (failure) {
      if (failure instanceof StartCancelled) {
        return "cancelled";
      }
      throw failure;
    }
    if (outcome.id === null || outcome.skipped) {
      return "skipped";
    }
    const id = outcome.id;
    // A rename changes where the file goes, so the row follows the engine.
    const written = outcome.dest || dest;
    const now = Date.now();
    const { dir, base } = splitPath(written);
    const row: Download = {
      id,
      fileName: base || input.fileName,
      url: input.url,
      category: input.category,
      size: 0,
      downloaded: 0,
      status: "queued",
      speed: 0,
      eta: 0,
      addedAt: now,
      lastTryAt: now,
      savePath: dir,
      dest: written,
      referer: input.referer?.trim() || null,
      connections,
      error: null,
      resumable: null,
      description: input.description?.trim() || null,
    };
    setState((current) => ({
      ...current,
      downloads: current.downloads.some((item) => item.id === id)
        ? current.downloads
        : [...current.downloads, row],
    }));
    return "started";
  }

  /**
   * A link the browser extension handed over.
   *
   * The browser waits while the user confirms it: the address and the name
   * the browser gave the file are put in the add dialog, which starts the
   * download when the user says so.
   */
  function takeFromBrowser(payload: BridgeDownload) {
    setBrowserRequest({
      url: payload.url,
      name: splitPath(payload.name).base,
      referer: payload.referer,
    });
  }


  return {
    conflict,
    browserRequest,
    clearBrowserRequest: () => setBrowserRequest(null),
    askConflict,
    settleConflict,
    restart,
    beginDownload,
    takeFromBrowser,
  };
}
