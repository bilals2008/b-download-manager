import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import { KeyCap } from "@/components/ui/key-cap";
import { Tooltip } from "@/components/ui/tooltip";
import { stepTipIndex, TIPS } from "@/lib/tips";

interface TipOfTheDayDialogProps {
  open: boolean;
  index: number;
  onIndexChange: (index: number) => void;
  onClose: () => void;
  /** Turn startup tips off for good; closing alone keeps them on. */
  onDisable: () => void;
}

/**
 * One tip at a time, the way the reference download manager offers them.
 *
 * Closing the window is a "not now": the next start offers another tip.
 * "Don't show tips again" is the permanent answer, and the Settings window is
 * where it can be undone. The two are deliberately different words as well as
 * different actions.
 */
export function TipOfTheDayDialog({
  open,
  index,
  onIndexChange,
  onClose,
  onDisable,
}: TipOfTheDayDialogProps) {
  // A saved index from a shorter collection still lands on a real tip.
  const current = stepTipIndex(index, 0);
  const tip = TIPS[current];

  return (
    <Dialog
      open={open}
      title="Tip of the Day"
      onClose={onClose}
      className="w-[min(26rem,calc(100vw-2rem))]"
      bodyClassName="px-5 pt-1 pb-4"
      footerClassName="items-center gap-1.5 px-5 pb-4 pt-1"
      footer={
        <>
          <Tooltip
            content="Closing keeps tips on for the next start. This turns them off until you re-enable them in Settings > Tips."
            side="top"
            className="mr-auto"
          >
            <DialogAction tone="quiet" onClick={onDisable}>
              Don't show tips again
            </DialogAction>
          </Tooltip>
          <DialogAction
            onClick={() => onIndexChange(stepTipIndex(current, -1))}
          >
            Previous
          </DialogAction>
          <DialogAction
            tone="primary"
            onClick={() => onIndexChange(stepTipIndex(current, 1))}
          >
            Next
          </DialogAction>
        </>
      }
    >
      <div className="flex min-h-28 flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <span className="text-chrome-micro text-chrome-muted">
            {tip.category}
          </span>
          <span className="shrink-0 text-chrome-micro tabular-nums text-chrome-muted">
            {current + 1} / {TIPS.length}
          </span>
        </div>
        <div aria-live="polite" className="flex flex-col gap-1.5">
          <h2 className="text-sm leading-5 font-medium text-chrome-title-text">
            {tip.title}
          </h2>
          <p className="text-xs leading-5 text-chrome-muted">{tip.body}</p>
          {tip.shortcut ? (
            <KeyCap className="mt-0.5 self-start">{tip.shortcut}</KeyCap>
          ) : null}
        </div>
      </div>
    </Dialog>
  );
}
