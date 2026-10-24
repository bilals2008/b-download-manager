export type TooltipSide = "top" | "bottom" | "left" | "right";

/** The part of a box the placement needs: viewport or element, same shape. */
export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface TooltipPlacement {
  x: number;
  y: number;
  /** The side the tooltip actually landed on, after flipping. */
  side: TooltipSide;
  /** Where the caret sits along the tooltip edge, in pixels. */
  arrow: number;
}

/** Distance between the trigger and the tooltip body: the caret plus air. */
export const TOOLTIP_GAP = 10;

const MARGIN = 8;
const ARROW_INSET = 16;

const OPPOSITE: Record<TooltipSide, TooltipSide> = {
  top: "bottom",
  bottom: "top",
  left: "right",
  right: "left",
};

/**
 * Picks where a tooltip sits over its trigger: the preferred side when it
 * fits, its opposite when it does not, and never outside `viewport`. The caret
 * follows the trigger's centre until that would push it off the edge.
 */
export function placeTooltip(
  preferred: TooltipSide,
  anchor: Box,
  tip: Box,
  viewport: { width: number; height: number },
): TooltipPlacement {
  const fits: Record<TooltipSide, boolean> = {
    top: anchor.top - tip.height - TOOLTIP_GAP >= MARGIN,
    bottom:
      anchor.top + anchor.height + tip.height + TOOLTIP_GAP <=
      viewport.height - MARGIN,
    left: anchor.left - tip.width - TOOLTIP_GAP >= MARGIN,
    right:
      anchor.left + anchor.width + tip.width + TOOLTIP_GAP <=
      viewport.width - MARGIN,
  };

  const side = fits[preferred] || !fits[OPPOSITE[preferred]] ? preferred : OPPOSITE[preferred];

  const centerX = anchor.left + anchor.width / 2;
  const centerY = anchor.top + anchor.height / 2;

  if (side === "top" || side === "bottom") {
    const x = clamp(
      centerX - tip.width / 2,
      MARGIN,
      viewport.width - tip.width - MARGIN,
    );
    const edge =
      side === "top"
        ? anchor.top - tip.height - TOOLTIP_GAP
        : anchor.top + anchor.height + TOOLTIP_GAP;
    const y = clamp(edge, MARGIN, viewport.height - tip.height - MARGIN);
    const inset = Math.min(ARROW_INSET, tip.width / 2);
    return { side, x, y, arrow: clamp(centerX - x, inset, tip.width - inset) };
  }

  const y = clamp(
    centerY - tip.height / 2,
    MARGIN,
    viewport.height - tip.height - MARGIN,
  );
  const edge =
    side === "left"
      ? anchor.left - tip.width - TOOLTIP_GAP
      : anchor.left + anchor.width + TOOLTIP_GAP;
  const x = clamp(edge, MARGIN, viewport.width - tip.width - MARGIN);
  const inset = Math.min(ARROW_INSET, tip.height / 2);
  return { side, x, y, arrow: clamp(centerY - y, inset, tip.height - inset) };
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), Math.max(min, max));
}
