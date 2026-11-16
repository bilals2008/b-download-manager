import { useEffect, useRef } from "react";

/** How wide a cell is, in pixels. */
const CELL = 6;
/** The gap between two cells, in pixels. */
const GAP = 1;
/** The distance from one cell to the next, in pixels. */
const PITCH = CELL + GAP;

interface PieceGridProps {
  /** The pieces that are on disk, as a bitmap in base64. */
  bits: string;
  /** How many pieces the torrent has. */
  count: number;
  /** The piece that is coming in right now, when one is. */
  inflight: number | null;
  /** The words the picture is given to a screen reader. */
  label: string;
}

/** The bytes a base64 bitmap stands for. */
function decode(bits: string): Uint8Array {
  try {
    const text = atob(bits);
    const bytes = new Uint8Array(text.length);
    for (let index = 0; index < text.length; index += 1) {
      bytes[index] = text.charCodeAt(index);
    }
    return bytes;
  } catch {
    // A bitmap the engine did not write says nothing, and the grid then
    // shows every piece as missing.
    return new Uint8Array(0);
  }
}

/** Whether the bitmap says the piece is on disk. */
function hasPiece(bytes: Uint8Array, index: number): boolean {
  const byte = bytes[index >> 3];
  if (byte === undefined) {
    return false;
  }
  return (byte & (0x80 >> (index & 7))) !== 0;
}

/**
 * The pieces of a torrent, one cell each.
 *
 * A torrent can hold twenty thousand pieces, so the cells are painted on a
 * canvas instead of being twenty thousand elements in the document: the grid
 * is one picture, it costs a frame, and the window never stops answering.
 *
 * The picture is given to a screen reader as one image with the counts in its
 * name, so nothing has to walk twenty thousand cells to learn what the
 * torrent has.
 */
export function PieceGrid({ bits, count, inflight, label }: PieceGridProps) {
  const frame = useRef<HTMLDivElement | null>(null);
  const canvas = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const element = canvas.current;
    const holder = frame.current;
    if (!element || !holder) {
      return;
    }

    const paint = () => {
      const context = element.getContext("2d");
      if (!context) {
        return;
      }
      const width = holder.clientWidth;
      if (width <= 0 || count <= 0) {
        return;
      }
      const columns = Math.max(1, Math.floor(width / PITCH));
      const rows = Math.ceil(count / columns);
      const ratio = window.devicePixelRatio || 1;

      element.style.width = `${columns * PITCH}px`;
      element.style.height = `${rows * PITCH}px`;
      element.width = Math.round(columns * PITCH * ratio);
      element.height = Math.round(rows * PITCH * ratio);
      context.setTransform(ratio, 0, 0, ratio, 0, 0);

      // The colours are the tokens of the appearance, so the grid follows the
      // theme the window is in.
      const style = getComputedStyle(document.documentElement);
      const accent = style.getPropertyValue("--chrome-focus").trim() || style.color;
      const empty = style.getPropertyValue("--chrome-line").trim() || style.color;
      const bytes = decode(bits);

      for (let index = 0; index < count; index += 1) {
        context.fillStyle = hasPiece(bytes, index) ? accent : empty;
        const column = index % columns;
        const row = Math.floor(index / columns);
        context.fillRect(column * PITCH, row * PITCH, CELL, CELL);
      }

      // The piece that is on its way is drawn in the accent at half strength:
      // it is there and it is not there, and it is the only cell like that.
      if (inflight !== null && inflight >= 0 && inflight < count) {
        context.globalAlpha = 0.55;
        context.fillStyle = accent;
        const column = inflight % columns;
        const row = Math.floor(inflight / columns);
        context.fillRect(column * PITCH, row * PITCH, CELL, CELL);
        context.globalAlpha = 1;
      }
    }

    // The grid is painted again when the window is resized and when the
    // appearance changes, so the colours never belong to another theme.
    const resize = new ResizeObserver(paint);
    resize.observe(holder);
    const appearance = new MutationObserver(paint);
    appearance.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    paint();

    return () => {
      resize.disconnect();
      appearance.disconnect();
    };
  }, [bits, count, inflight]);

  return (
    <div ref={frame} role="img" aria-label={label} className="w-full">
      <canvas ref={canvas} aria-hidden="true" className="block" />
    </div>
  );
}
