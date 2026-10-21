import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";

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

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onCancel={onClose}
      aria-label={title}
      className={cn(
        // `open:flex` matters: a plain `flex` would override the native
        // `dialog:not([open]) { display: none }` rule and every dialog would
        // render at once.
        "m-auto w-96 flex-col overflow-hidden rounded-chrome border border-chrome-line bg-chrome-popup p-0 text-chrome-menu-text backdrop:bg-black/60 open:flex",
        className,
      )}
    >
      <div className="flex shrink-0 items-center justify-between gap-4 border-b border-chrome-line py-1.5 pr-2 pl-4">
        <span className="truncate text-sm text-chrome-title-text">{title}</span>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close"
          className="flex size-6 shrink-0 items-center justify-center rounded-chrome text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          <X className="size-3.5" aria-hidden="true" />
        </button>
      </div>
      <div className={cn("min-h-0 flex-1 px-4 py-3", bodyClassName)}>
        {children}
      </div>
      {footer ? (
        <div className="flex shrink-0 justify-end gap-2 border-t border-chrome-line px-4 py-2">
          {footer}
        </div>
      ) : null}
    </dialog>
  );
}
