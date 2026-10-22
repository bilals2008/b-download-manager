import {
  useEffect,
  useRef,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";

import type { SelectMode } from "@/state/download-store";

interface DragState {
  /** The row the pointer is over, so the range is extended only on a change. */
  hover: number;
  moved: boolean;
  x: number;
  y: number;
}

/** How close to the top or bottom edge the pointer starts scrolling. */
const EDGE = 28;
/** The sticky header covers the pointer at the top edge, so aim below it. */
const HEADER_CLEARANCE = 32;

/**
 * Picking a range by dragging across the download rows, the way Explorer does.
 *
 * The move and release listeners live on the window, so a drag that leaves the
 * table keeps working. Near the top or bottom edge the list scrolls on its own
 * and the selection follows the rows that pass under the pointer. The click
 * that ends a real drag is swallowed, so it cannot collapse the range to the
 * row it lands on.
 */
export function useDragSelect({
  scrollRef,
  onSelect,
}: {
  scrollRef: RefObject<HTMLDivElement | null>;
  onSelect: (id: number, mode: SelectMode) => void;
}) {
  const drag = useRef<DragState | null>(null);
  const frame = useRef<number | null>(null);
  const suppressClick = useRef(false);
  const choose = useRef(onSelect);
  choose.current = onSelect;

  function extend(current: DragState, y = current.y) {
    const id = rowIdAt(document.elementFromPoint(current.x, y));
    if (id === null || id === current.hover) {
      return;
    }
    current.hover = id;
    current.moved = true;
    choose.current(id, "range");
  }

  useEffect(() => {
    function stop() {
      if (frame.current !== null) {
        cancelAnimationFrame(frame.current);
        frame.current = null;
      }
    }

    function step() {
      frame.current = null;
      const current = drag.current;
      const box = scrollRef.current;
      if (!current || !box) {
        return;
      }
      const rect = box.getBoundingClientRect();
      const delta =
        current.y < rect.top + EDGE
          ? -Math.ceil(EDGE - (current.y - rect.top))
          : current.y > rect.bottom - EDGE
            ? Math.ceil(EDGE - (rect.bottom - current.y))
            : 0;
      if (delta === 0) {
        return;
      }
      const before = box.scrollTop;
      const speed = Math.min(24, Math.max(6, Math.abs(delta)));
      box.scrollTop = before + speed * Math.sign(delta);
      if (box.scrollTop === before) {
        return;
      }
      // shortcut: the sticky header hides the row at the top edge, so the
      // target is read just below it. Revisit if the header height changes.
      const y =
        current.y < rect.top + EDGE
          ? rect.top + HEADER_CLEARANCE
          : Math.min(current.y, rect.bottom - 2);
      extend(current, y);
      frame.current = requestAnimationFrame(step);
    }

    function move(event: PointerEvent) {
      const current = drag.current;
      if (!current) {
        return;
      }
      if (event.buttons === 0) {
        drag.current = null;
        stop();
        return;
      }
      current.x = event.clientX;
      current.y = event.clientY;
      extend(current);

      const box = scrollRef.current;
      if (!box) {
        return;
      }
      const rect = box.getBoundingClientRect();
      const inEdge =
        current.x >= rect.left &&
        current.x <= rect.right &&
        (current.y < rect.top + EDGE || current.y > rect.bottom - EDGE);
      if (inEdge && frame.current === null) {
        frame.current = requestAnimationFrame(step);
      }
    }

    function end() {
      if (drag.current?.moved) {
        suppressClick.current = true;
      }
      drag.current = null;
      stop();
    }

    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    // A release outside the window never reports a pointerup here; losing the
    // focus is the only hint, and a stuck loop would keep scrolling.
    window.addEventListener("blur", end);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("blur", end);
      stop();
    };
  }, [scrollRef]);

  /** A press on a row starts a drag; modifier clicks are left to the click. */
  function start(event: ReactPointerEvent) {
    if (event.button !== 0 || event.shiftKey || event.ctrlKey || event.metaKey) {
      return;
    }
    if (
      event.target instanceof Element &&
      event.target.closest("input, [role='separator']")
    ) {
      return;
    }
    const id = rowIdAt(event.target instanceof Element ? event.target : null);
    if (id === null) {
      return;
    }
    suppressClick.current = false;
    drag.current = {
      hover: id,
      moved: false,
      x: event.clientX,
      y: event.clientY,
    };
    choose.current(id, "replace");
  }

  /** True when the click belongs to the end of a drag and must be ignored. */
  function consumeClick(): boolean {
    if (!suppressClick.current) {
      return false;
    }
    suppressClick.current = false;
    return true;
  }

  return { start, consumeClick };
}

/** The download id of the row a node belongs to, if any. */
function rowIdAt(node: Element | null): number | null {
  const row = node?.closest<HTMLElement>("[data-download-id]") ?? null;
  const id = Number(row?.dataset.downloadId);
  return Number.isInteger(id) ? id : null;
}
