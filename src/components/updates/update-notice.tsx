import { AppMark } from "@/components/shell/app-mark";
import { CloseGlyph } from "@/components/icons/ui-icons";
import { DialogAction } from "@/components/ui/dialog-action";
import { UpdateProgressBody } from "@/components/updates/update-meter";
import type { UpdateFound } from "@/lib/updates";
import {
  beginUpdate,
  clearUpdateRun,
  restartApplication,
  retryUpdate,
  useUpdateRun,
} from "@/state/update-store";

interface UpdateNoticeProps {
  /** A release the startup check found, before the reader answered. */
  offered: UpdateFound | null;
  /** The reader put this release off, so the same one is not offered again. */
  onLater: (version: string) => void;
  /** The offer became a download. */
  onAccepted: () => void;
}

/**
 * The small surface a newer release announces itself on.
 *
 * It is not a window: it does not take the keyboard, it does not cover the
 * list, and it never appears for a release the reader already put off. Once a
 * download starts the same card becomes its real progress, which is why the
 * stage, the bytes and the bar come from the backend instead of a timer.
 */
export function UpdateNotice({
  offered,
  onLater,
  onAccepted,
}: UpdateNoticeProps) {
  const run = useUpdateRun();
  const running =
    run.phase === "downloading" ||
    run.phase === "verifying" ||
    run.phase === "installing";
  const finished = run.phase === "restart-ready";
  const failed = run.phase === "error";

  if (!offered && !running && !finished && !failed) {
    return null;
  }

  const version = run.update?.version ?? offered?.version ?? "";
  const current = run.update?.currentVersion ?? offered?.currentVersion ?? "";

  // A failure is also a "not now": leaving it should not drop the reader
  // straight back onto the offer for the same release.
  function leave(): void {
    clearUpdateRun();
    if (failed && offered) {
      onLater(offered.version ?? "");
    }
  }

  return (
    <div
      role="region"
      aria-label="Application update"
      className="fixed right-4 bottom-9 z-50 w-[21rem] rounded-lg bg-chrome-popup p-3 text-chrome-menu-text shadow-float motion-safe:animate-details-in motion-reduce:animate-none"
    >
      <div className="flex items-start gap-2.5">
        <AppMark className="mt-0.5 size-5" />
        <div className="min-w-0 flex-1">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p
                className="text-xs font-medium text-chrome-title-text"
                aria-live={failed ? "assertive" : "polite"}
              >
                {failed
                  ? "The update could not be installed"
                  : finished
                    ? "Update ready"
                    : running
                      ? `Updating to ${version}`
                      : "Update available"}
              </p>
              {!failed ? (
                <p className="mt-0.5 text-chrome-micro text-chrome-muted">
                  {running || finished
                    ? `Version ${version}, from ${current}.`
                    : `Version ${version} is available. You are on ${current}.`}
                </p>
              ) : null}
            </div>
            {!running ? (
              <button
                type="button"
                aria-label={finished || failed ? "Close" : "Remind me later"}
                onClick={() => {
                  if (finished || failed) {
                    leave();
                  } else if (offered) {
                    onLater(offered.version ?? "");
                  }
                }}
                className="-mt-1 -mr-1 flex size-6 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
              >
                <CloseGlyph className="size-3" />
              </button>
            ) : null}
          </div>

          {offered && !running && !finished && !failed ? (
            <div className="mt-2.5 flex flex-wrap gap-2">
              <DialogAction
                tone="primary"
                onClick={() => {
                  beginUpdate(offered);
                  onAccepted();
                }}
              >
                Update now
              </DialogAction>
              <DialogAction
                tone="quiet"
                onClick={() => onLater(offered.version ?? "")}
              >
                Later
              </DialogAction>
            </div>
          ) : null}

          {running || finished || failed ? (
            <div>
              <UpdateProgressBody run={run} className="mt-2" />
              {failed ? (
                <div className="mt-2.5 flex flex-wrap gap-2">
                  <DialogAction tone="primary" onClick={retryUpdate}>
                    Try again
                  </DialogAction>
                  <DialogAction tone="quiet" onClick={leave}>
                    Later
                  </DialogAction>
                </div>
              ) : null}
              {finished ? (
                <div className="mt-2.5 flex flex-wrap gap-2">
                  <DialogAction tone="primary" onClick={restartApplication}>
                    Restart now
                  </DialogAction>
                  <DialogAction tone="quiet" onClick={clearUpdateRun}>
                    Later
                  </DialogAction>
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
