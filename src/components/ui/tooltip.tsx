import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

import { placeTooltip, type TooltipSide } from "@/lib/tooltip-position";
import { cn } from "@/lib/utils";

export type { TooltipSide };

interface TooltipProps {
  /** What the tooltip says: a line of text or a small node. */
  content: ReactNode;
  /** The control it describes. It keeps its own handlers and refs. */
  children: ReactNode;
  /** Where it opens before it flips itself to fit the window. */
  side?: TooltipSide;
  /** How long the pointer must rest, in milliseconds; focus ignores it. */
  delay?: number;
  /** An accelerator, printed on a key cap after the text. */
  shortcut?: string;
  /** Extra classes for the anchor, such as `shrink-0` in a flex row. */
  className?: string;
}

/** How long the next tooltip stays warm after one closes. */
const WARM_MS = 300;

// Moving along a toolbar should feel like one instrument, not eleven timers:
// once a tooltip has shown, its neighbours open at once.
let warmUntil = 0;

/**
 * A tooltip that belongs to the window rather than to its trigger: it is
 * portaled to the body, so a scrolling toolbar never clips it, and it flips
 * and clamps itself to stay inside the window. Hover waits, keyboard focus
 * does not, and Escape dismisses it.
 */
export function Tooltip({
  content,
  children,
  side = "top",
  delay = 320,
  shortcut,
  className,
}: TooltipProps) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const frame = useRef<number | undefined>(undefined);
  const [open, setOpen] = useState(false);
  // The portal is built on first use, so a list of downloadable rows does not
  // carry a hidden tooltip per row in the document.
  const [mounted, setMounted] = useState(false);
  // A native dialog lives in the top layer: a portal in the body would paint
  // behind it, so a tooltip inside one is hosted by that dialog instead.
  const [host, setHost] = useState<HTMLElement | null>(null);
  const [spot, setSpot] = useState<ReturnType<typeof placeTooltip> | null>(
    null,
  );

  const hasContent =
    content !== null && content !== undefined && content !== "";

  function reveal() {
    window.clearTimeout(timer.current);
    window.cancelAnimationFrame(frame.current ?? 0);
    const anchor = anchorRef.current;
    if (!anchor || !hasContent) {
      return;
    }
    if (!mounted) {
      setHost(anchor.closest("dialog") ?? document.body);
      setMounted(true);
    }
    if (mounted) {
      setOpen(true);
      return;
    }
    // Let the hidden node paint once, or the first open has nothing to
    // transition from; the second frame is where the entrance starts.
    frame.current = window.requestAnimationFrame(() => {
      frame.current = window.requestAnimationFrame(() => setOpen(true));
    });
  }

  function openSoon() {
    window.clearTimeout(timer.current);
    if (!hasContent) {
      return;
    }
    if (performance.now() < warmUntil) {
      reveal();
      return;
    }
    timer.current = window.setTimeout(reveal, delay);
  }

  function close() {
    window.clearTimeout(timer.current);
    window.cancelAnimationFrame(frame.current ?? 0);
    if (open) {
      warmUntil = performance.now() + WARM_MS;
    }
    setOpen(false);
  }

  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
      window.cancelAnimationFrame(frame.current ?? 0);
    },
    [],
  );

  // The tooltip is measured hidden, then placed before the frame is painted,
  // so it never flashes at the corner. It follows the trigger while the
  // window resizes or scrolls, and leaves when the trigger leaves the window.
  useLayoutEffect(() => {
    if (!open) {
      return;
    }
    const anchor = anchorRef.current;
    const tip = tipRef.current;
    if (!anchor || !tip) {
      return;
    }

    function place() {
      if (!anchor || !tip) {
        return;
      }
      const box = anchor.getBoundingClientRect();
      const viewport = { width: window.innerWidth, height: window.innerHeight };
      if (
        box.bottom < 0 ||
        box.top > viewport.height ||
        box.right < 0 ||
        box.left > viewport.width
      ) {
        setOpen(false);
        return;
      }
      const next = placeTooltip(
        side,
        box,
        // The entrance transform scales the box, so measure its layout size
        // rather than its painted rect.
        { left: 0, top: 0, width: tip.offsetWidth, height: tip.offsetHeight },
        viewport,
      );
      setSpot((current) =>
        current &&
        current.x === next.x &&
        current.y === next.y &&
        current.side === next.side &&
        current.arrow === next.arrow
          ? current
          : next,
      );
    }

    place();
    window.addEventListener("resize", place);
    // Like the menus, a scroll dismisses the tooltip instead of dragging it
    // along: no per-event measuring or re-rendering while the list moves.
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", close, true);
    };
  }, [open, side]);

  // Escape dismisses the tooltip without moving focus or taking the click.
  useEffect(() => {
    if (!open) {
      return;
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        close();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open]);

  const placement = spot?.side ?? side;

  return (
    <>
      <span
        ref={anchorRef}
        className={cn("inline-flex", className)}
        onPointerEnter={(event) => {
          if (event.pointerType !== "touch") {
            openSoon();
          }
        }}
        onPointerLeave={close}
        onPointerDown={close}
        onFocus={reveal}
        onBlur={close}
      >
        {children}
      </span>
      {mounted && host
        ? createPortal(
            <div
              ref={tipRef}
              role="tooltip"
              aria-hidden={open ? undefined : true}
              data-slot="tooltip"
              data-side={placement}
              data-open={open || undefined}
              style={{ left: spot?.x ?? 0, top: spot?.y ?? 0 }}
              onTransitionEnd={(event) => {
                // The transform is the longest part of the leave; wait for it
                // before dropping the node, or the exit cuts short.
                if (
                  !open &&
                  event.target === tipRef.current &&
                  event.propertyName === "transform"
                ) {
                  setMounted(false);
                }
              }}
              className="pointer-events-none fixed z-50 flex w-max max-w-64 items-center gap-2 rounded-md border border-chrome-line bg-chrome-popup px-2.5 py-1.5 text-xs leading-snug text-chrome-menu-text shadow-float select-none"
            >
              <span className="min-w-0 break-words">{content}</span>
              {shortcut ? (
                <kbd className="shrink-0 rounded-sm border border-chrome-line bg-chrome-toolbar-active px-1.5 font-mono text-chrome-micro text-chrome-muted">
                  {shortcut}
                </kbd>
              ) : null}
              <span
                aria-hidden="true"
                style={
                  placement === "top" || placement === "bottom"
                    ? { left: spot?.arrow }
                    : { top: spot?.arrow }
                }
                className={cn(
                  "absolute size-2 rotate-45 border-chrome-line bg-chrome-popup",
                  CARET[placement],
                )}
              />
            </div>,
            host,
          )
        : null}
    </>
  );
}

/** The half of the caret that clears the tooltip, per side. */
const CARET: Record<TooltipSide, string> = {
  top: "-bottom-[5px] -translate-x-1/2 border-r border-b",
  bottom: "-top-[5px] -translate-x-1/2 border-t border-l",
  left: "-right-[5px] -translate-y-1/2 border-t border-r",
  right: "-left-[5px] -translate-y-1/2 border-b border-l",
};
