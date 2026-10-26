import { useEffect, useMemo, useState } from "react";

import { CloseGlyph, DownloadGlyph } from "@/components/icons/ui-icons";
import { DialogAction } from "@/components/ui/dialog-action";
import { Tooltip } from "@/components/ui/tooltip";
import { GrabberForm } from "@/components/windows/grabber/grabber-form";
import { GrabberResults } from "@/components/windows/grabber/grabber-results";
import { useGrabber } from "@/components/windows/grabber/use-grabber";
import { splitPath } from "@/lib/downloads";
import {
  chosenFiles,
  folderProblem as checkFolder,
  grabberAddressProblem,
  isRunning,
  loadGrabberPreferences,
  optionsFrom,
  progressLine,
  saveGrabberPreferences,
  visibleFiles,
  type GrabberKind,
  type GrabberPreferences,
} from "@/lib/grabber";
import { chooseFolder, defaultDestination } from "@/lib/ipc";
import { closeWindow, showWindowWhenPainted } from "@/lib/window";

/**
 * The Site Grabber window.
 *
 * One site is read on request, the pages it links to within the limits on the
 * left, and the files those pages offer are listed on the right. Nothing is
 * downloaded until a file is ticked and the download button is pressed; those
 * downloads are ordinary rows in the main list from that moment on.
 *
 * The window can be closed while a crawl runs. The backend keeps reading and
 * keeps its snapshot, so reopening the window shows the crawl where it is.
 */
export function GrabberWindow() {
  const grabber = useGrabber();
  const [prefs, setPrefs] = useState<GrabberPreferences>(loadGrabberPreferences);
  const [url, setUrl] = useState("");
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [kind, setKind] = useState<GrabberKind | "all">("all");
  const [query, setQuery] = useState("");

  const snapshot = grabber.snapshot;
  const running = isRunning(snapshot);
  const problem = url.trim() ? grabberAddressProblem(url) : null;
  const folderIssue = prefs.folder ? checkFolder(prefs.folder) : null;

  const files = useMemo(
    () => visibleFiles(snapshot.files, kind, query),
    [snapshot.files, kind, query],
  );
  // Every answered link stays out of the list: answering one twice would only
  // add a second row of the same file, or repeat the refusal.
  const sent = useMemo(
    () => new Set(grabber.results.map((result) => result.url)),
    [grabber.results],
  );
  const chosen = chosenFiles(snapshot.files, selected, sent);

  useEffect(() => {
    void showWindowWhenPainted();
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape" && !event.defaultPrevented) {
        void closeWindow();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // The folder starts where the last download went, the way the add dialog
  // does, and falls back to the system Downloads folder.
  useEffect(() => {
    if (prefs.folder) {
      return;
    }
    let active = true;
    void defaultDestination()
      .then((answer) => {
        const { dir } = splitPath(answer);
        if (active && dir) {
          setPrefs((current) =>
            current.folder ? current : { ...current, folder: dir },
          );
        }
      })
      .catch(() => {
        // The folder picker still works when the engine cannot answer.
      });
    return () => {
      active = false;
    };
    // Only the first open needs a default; a folder the user picked is theirs.
  }, []);

  useEffect(() => {
    saveGrabberPreferences(prefs);
  }, [prefs]);

  function startCrawl() {
    setSelected(new Set());
    setQuery("");
    void grabber.start(optionsFrom(url, prefs));
  }

  async function pickFolder() {
    try {
      const picked = await chooseFolder(prefs.folder);
      if (picked) {
        setPrefs((current) => ({ ...current, folder: picked }));
      }
    } catch {
      // The dialog was cancelled or the system refused it; the old folder
      // stays, and the button can be pressed again.
    }
  }

  function toggleFile(target: string, checked: boolean) {
    setSelected((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(target);
      } else {
        next.delete(target);
      }
      return next;
    });
  }

  function selectShown() {
    setSelected((current) => {
      const next = new Set(current);
      for (const file of files) {
        if (!sent.has(file.url)) {
          next.add(file.url);
        }
      }
      return next;
    });
  }

  function clearSelection() {
    setSelected(new Set());
  }

  function download() {
    if (!chosen.length) {
      return;
    }
    void grabber.send(chosen, {
      folder: prefs.folder,
      connections: prefs.connections,
      queue: prefs.queue,
    });
  }

  return (
    <div className="flex h-dvh flex-col overflow-hidden rounded-lg bg-chrome-popup text-chrome-menu-text">
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center justify-between gap-4 border-b border-chrome-line py-2 pr-2 pl-4"
      >
        <div
          data-tauri-drag-region
          className="flex min-w-0 items-baseline gap-2"
        >
          <span className="shrink-0 text-chrome-micro font-medium tracking-wide text-chrome-muted uppercase">
            Site Grabber
          </span>
          {snapshot.site ? (
            <Tooltip content={snapshot.url} side="bottom" className="min-w-0">
              <span className="block truncate font-mono text-chrome-micro text-chrome-muted">
                {snapshot.site}
              </span>
            </Tooltip>
          ) : null}
        </div>
        <button
          type="button"
          onClick={() => void closeWindow()}
          aria-label="Close"
          className="flex size-7 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          <CloseGlyph className="size-3.5" />
        </button>
      </div>

      <div className="flex min-h-0 flex-1">
        <GrabberForm
          url={url}
          onUrlChange={setUrl}
          problem={problem}
          prefs={prefs}
          onPrefsChange={setPrefs}
          folderProblem={folderIssue}
          running={running}
          busy={grabber.busy === "start"}
          onStart={startCrawl}
          onPickFolder={() => void pickFolder()}
        />
        <GrabberResults
          snapshot={snapshot}
          files={files}
          totalFiles={snapshot.files.length}
          kind={kind}
          onKindChange={setKind}
          query={query}
          onQueryChange={setQuery}
          selected={selected}
          sent={sent}
          onToggle={toggleFile}
          onSelectShown={selectShown}
          onClear={clearSelection}
          results={grabber.results}
        />
      </div>

      {grabber.error ? (
        <div className="flex shrink-0 items-start gap-2 border-t border-chrome-line bg-chrome-canvas px-4 py-2">
          <p
            role="alert"
            className="min-w-0 flex-1 text-chrome-micro break-words text-chrome-error"
          >
            {grabber.error}
          </p>
          <button
            type="button"
            onClick={grabber.clearError}
            className="shrink-0 rounded-chrome px-1.5 py-0.5 text-chrome-micro text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none"
          >
            Dismiss
          </button>
        </div>
      ) : null}

      <footer className="flex shrink-0 items-center gap-3 border-t border-chrome-line px-4 py-2.5">
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs text-chrome-menu-text">
            {progressLine(snapshot)}
          </p>
          <p className="truncate text-chrome-micro text-chrome-muted">
            {chosen.length
              ? `${chosen.length} selected · the rest stay unticked`
              : "Files start unticked; choose what you want to keep."}
          </p>
        </div>

        {running ? (
          <DialogAction
            tone="secondary"
            disabled={snapshot.status === "cancelling"}
            onClick={() => void grabber.cancel()}
          >
            {snapshot.status === "cancelling" ? "Stopping…" : "Stop"}
          </DialogAction>
        ) : null}

        <DialogAction
          tone="primary"
          icon={<DownloadGlyph className="size-3.5" />}
          disabled={!chosen.length || grabber.busy !== null}
          onClick={download}
        >
          {grabber.busy === "send"
            ? "Adding…"
            : chosen.length
              ? `Download ${chosen.length} file${chosen.length === 1 ? "" : "s"}`
              : "Download files"}
        </DialogAction>
      </footer>
    </div>
  );
}
