import { AppMark } from "@/components/shell/app-mark";
import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { APP_NAME } from "@/lib/brand";
import type { WhatsNewRelease } from "@/lib/whats-new";

interface WhatsNewDialogProps {
  open: boolean;
  release: WhatsNewRelease | null;
  /** True when the reader asked for the notes rather than arriving from an update. */
  manual: boolean;
  onClose: () => void;
}

/**
 * The notes for one release, in the words the file was written in.
 *
 * This is a reader's box, not a changelog: the mark is the application's, the
 * version and day sit together, and each friendly group is a short list. It
 * is also where a reader lands from Settings > About after dismissing it.
 */
export function WhatsNewDialog({
  open,
  release,
  manual,
  onClose,
}: WhatsNewDialogProps) {
  return (
    <Dialog
      open={open}
      title="What's new"
      onClose={onClose}
      className="w-[min(32rem,calc(100vw-2rem))]"
      bodyClassName="pb-1"
      footerClassName="items-center gap-2 px-4 py-2.5"
      footer={
        <>
          {manual ? null : (
            <span className="mr-auto text-chrome-micro text-chrome-muted">
              Find this again in Settings &gt; About.
            </span>
          )}
          <DialogAction tone="primary" data-autofocus onClick={onClose}>
            Got it
          </DialogAction>
        </>
      }
    >
      {release ? (
        <div className="flex min-h-0 flex-col gap-4">
          <header className="flex items-center gap-3">
            <AppMark className="size-10" />
            <div className="min-w-0">
              <p className="text-chrome-micro leading-4 text-chrome-muted">
                {APP_NAME}
              </p>
              <h2 className="text-base leading-6 text-chrome-title-text">
                Version {release.version}
              </h2>
              {release.date ? (
                <p className="text-chrome-micro leading-4 text-chrome-muted">
                  {release.date}
                </p>
              ) : null}
            </div>
          </header>

          <div
            tabIndex={0}
            aria-label="Release highlights"
            className="max-h-[min(24rem,calc(100dvh-15rem))] overflow-y-auto scrollbar-thin pr-1 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            <div className="flex flex-col gap-4">
              {release.intro.map((paragraph, index) => (
                <p
                  key={`intro-${index}`}
                  className="text-xs leading-5 text-chrome-menu-text"
                >
                  {paragraph}
                </p>
              ))}
              {release.sections.map((section, index) => (
                <section key={index} className="flex flex-col gap-1.5">
                  {section.title ? (
                    <h3 className="text-xs leading-5 font-medium text-chrome-title-text">
                      {section.title}
                    </h3>
                  ) : null}
                  <ul className="flex flex-col gap-1.5">
                    {section.items.map((item, itemIndex) => (
                      <li
                        key={`${index}-${itemIndex}`}
                        className="flex gap-2.5 text-xs leading-5 text-chrome-menu-text"
                      >
                        <span
                          aria-hidden="true"
                          className="mt-1.5 size-1 shrink-0 rounded-full bg-primary"
                        />
                        <span className="min-w-0">{item}</span>
                      </li>
                    ))}
                  </ul>
                </section>
              ))}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex flex-col gap-3 py-2">
          <AppMark className="size-10" />
          <p className="text-xs leading-5 text-chrome-menu-text">
            There are no release notes for this version yet.
          </p>
        </div>
      )}
    </Dialog>
  );
}
