import { useEffect, useRef, type ReactNode } from "react";

import { CloseGlyph } from "@/components/icons/ui-icons";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

interface DialogProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  /** Width and height overrides for a dialog that is more than a small form. */
  className?: string;
  /** Padding and layout overrides for the body. */
  bodyClassName?: string;
  /** Overrides for the title row, for a dialog that is a window with a bar. */
  headerClassName?: string;
  /** Overrides for the button row. */
  footerClassName?: string;
  /**
   * Whether Escape and the close button may close it. A dialog that is
   * finishing a command turns this off, so the command cannot be left in
   * flight with nothing to explain it.
   */
  dismissible?: boolean;
  /**
   * Whether the title and the close button carry tooltips. A window that is
   * itself the title bar turns them off: nothing there needs explaining.
   */
  tooltips?: boolean;
}

/**
 * A modal built on the native `dialog` element, so focus trapping, Escape and
 * the inert background come from the platform instead of a re-implementation.
 */
export function Dialog({
  open,
  title,
  onClose,
  children,
  footer,
  className,
  bodyClassName,
  headerClassName,
  footerClassName,
  dismissible = true,
  tooltips = true,
}: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element) {
      return;
    }
    if (open && !element.open) {
      element.showModal();
    }
    if (!open && element.open) {
      element.close();
    }
  }, [open]);

  const closeButton = (
    <button
      type="button"
      onClick={onClose}
      aria-label="Close"
      disabled={!dismissible}
      className="flex size-7 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-chrome-muted"
    >
      <CloseGlyph className="size-3.5" />
    </button>
  );

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onCancel={(event) => {
        if (!dismissible) {
          event.preventDefault();
          return;
        }
        onClose();
      }}
      aria-label={title}
      className={cn(
        // `open:flex` matters: a plain `flex` would override the native
        // `dialog:not([open]) { display: none }` rule and every dialog would
        // render at once.
        "m-auto w-96 flex-col overflow-hidden rounded-lg bg-chrome-popup p-0 text-chrome-menu-text shadow-float backdrop:bg-black/60 open:flex",
        className,
      )}
    >
      <div
        className={cn(
          "flex shrink-0 items-center justify-between gap-4 py-3 pr-2 pl-4",
          headerClassName,
        )}
      >
        {tooltips ? (
          <Tooltip content={title} side="bottom" className="min-w-0 flex-1">
            <span className="block truncate text-sm text-chrome-title-text">
              {title}
            </span>
          </Tooltip>
        ) : (
          <div className="min-w-0 flex-1">
            <span className="block truncate text-sm text-chrome-title-text">
              {title}
            </span>
          </div>
        )}
        {closeButton}
      </div>
      <div className={cn("min-h-0 flex-1 px-4 pb-3", bodyClassName)}>
        {children}
      </div>
      {footer ? (
        <div
          className={cn(
            "flex shrink-0 justify-end gap-2 px-4 pb-4",
            footerClassName,
          )}
        >
          {footer}
        </div>
      ) : null}
    </dialog>
  );
}
