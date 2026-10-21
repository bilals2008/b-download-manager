import { useEffect, useRef, type ReactNode } from "react";

interface DialogProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
}

/**
 * A modal built on the native `dialog` element, so focus trapping, Escape and
 * the inert background come from the platform instead of a re-implementation.
 */
export function Dialog({ open, title, onClose, children, footer }: DialogProps) {
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
      className="m-auto w-96 rounded-chrome border border-chrome-line bg-chrome-popup p-0 text-chrome-menu-text backdrop:bg-black/60"
    >
      <div className="border-b border-chrome-line px-4 py-2 text-sm text-chrome-title-text">
        {title}
      </div>
      <div className="px-4 py-3">{children}</div>
      {footer ? (
        <div className="flex justify-end gap-2 border-t border-chrome-line px-4 py-2">
          {footer}
        </div>
      ) : null}
    </dialog>
  );
}
