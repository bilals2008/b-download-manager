/**
 * The Isometric and Blueprint toolbar artwork.
 *
 * Drawn in the fun-isometric idiom: every body is a chunky slab with a thick
 * ink outline and a flat side, the subject sits in a square, and colour comes
 * from palette slots rather than fixed values. The same geometry is painted
 * twice: once in the kit's RISO inks, once in its blue mono palette.
 */

import { toolbarIconDataUrl } from "./toolbar-art.ts";
import type { ToolbarCommand } from "./toolbars.ts";

interface SlabPalette {
  main: string;
  gold: string;
  tint: string;
  ok: string;
  hot: string;
  deep: string;
  white: string;
  grey: string;
  paper: string;
  ink: string;
}

/** The skill's RISO palette, slot for slot. */
const RISO: SlabPalette = {
  main: "#ff58a8",
  gold: "#ffe14a",
  tint: "#ffd4e8",
  ok: "#15b9a6",
  hot: "#ff7a1a",
  deep: "#2b50e0",
  white: "#ffffff",
  grey: "#cccccc",
  paper: "#f7f7f7",
  ink: "#000000",
};

/** `mono('#2b50e0')` from the kit: one hue at six lightnesses. */
const BLUEPRINT: SlabPalette = {
  main: "#5682ff",
  gold: "#a5c0ff",
  tint: "#e2ebff",
  ok: "#81a2f2",
  hot: "#3f66dd",
  deep: "#1f3dcd",
  white: "#ffffff",
  grey: "#cccccc",
  paper: "#f7f7f7",
  ink: "#000000",
};

const round = (value: number): number => Math.round(value * 100) / 100;

function rect(x: number, y: number, w: number, h: number, r = 0): string {
  if (r <= 0) {
    return `M${x} ${y}H${x + w}V${y + h}H${x}Z`;
  }
  return (
    `M${x + r} ${y}H${x + w - r}` +
    `A${r} ${r} 0 0 1 ${x + w} ${y + r}V${y + h - r}` +
    `A${r} ${r} 0 0 1 ${x + w - r} ${y + h}H${x + r}` +
    `A${r} ${r} 0 0 1 ${x} ${y + h - r}V${y + r}` +
    `A${r} ${r} 0 0 1 ${x + r} ${y}Z`
  );
}

function circle(cx: number, cy: number, r: number): string {
  return `M${cx - r} ${cy}A${r} ${r} 0 1 0 ${cx + r} ${cy}A${r} ${r} 0 1 0 ${cx - r} ${cy}Z`;
}

function poly(points: ReadonlyArray<readonly [number, number]>): string {
  return `M${points.map(([x, y]) => `${round(x)} ${round(y)}`).join("L")}Z`;
}

/** A cog silhouette: a square-toothed wheel, one path so it extrudes as one slab. */
function cog(cx: number, cy: number, outer: number, root: number, teeth = 8): string {
  const step = (Math.PI * 2) / teeth;
  const points: Array<[number, number]> = [];
  for (let index = 0; index < teeth; index += 1) {
    const angle = index * step;
    for (const [turn, radius] of [
      [0, root],
      [0.12, outer],
      [0.38, outer],
      [0.5, root],
    ] as const) {
      points.push([
        cx + Math.cos(angle + step * turn) * radius,
        cy + Math.sin(angle + step * turn) * radius,
      ]);
    }
  }
  return poly(points);
}

function paint(p: SlabPalette): Record<ToolbarCommand, string> {
  const line = 3;

  /** A body: the same silhouette behind in the side colour, then the face. */
  const slab = (
    shape: string,
    fill: string,
    side: string,
    dx = 6,
    dy = 6,
  ): string =>
    `<path d="${shape}" transform="translate(${dx} ${dy})" fill="${side}" stroke="${p.ink}" stroke-width="${line}" stroke-linejoin="round"/>` +
    `<path d="${shape}" fill="${fill}" stroke="${p.ink}" stroke-width="${line}" stroke-linejoin="round"/>`;

  const frame = (d: string, fill: string, width = line): string =>
    `<path d="${d}" fill="${fill}" stroke="${p.ink}" stroke-width="${width}" stroke-linejoin="round"/>`;

  const detail = (d: string, color = p.ink, width = line): string =>
    `<path d="${d}" fill="none" stroke="${color}" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round"/>`;

  const data = (body: string): string =>
    toolbarIconDataUrl(body, "", "0 0 64 64");

  return {
    "add-download": data(
      `${slab("M8 20v-6a5 5 0 0 1 5-5h12l5 6h16a5 5 0 0 1 5 5v28a5 5 0 0 1-5 5H13a5 5 0 0 1-5-5Z", p.main, p.deep)}
${frame(rect(15, 18, 30, 18, 2), p.paper, 2.5)}
${slab(rect(8, 32, 44, 18, 5), p.tint, p.main, 5, 5)}
${slab(circle(49, 47, 8), p.ok, p.deep, 3, 3)}
${detail("M49 43v8M45 47h8", p.white, 3.4)}`,
    ),

    resume: data(
      `${slab(rect(10, 12, 44, 34, 8), p.deep, p.main)}
${frame(rect(16, 18, 32, 22, 4), p.paper, 2.5)}
${frame(poly([[26, 22], [40, 29], [26, 36]]), p.ok, 3)}`,
    ),

    stop: data(
      `${slab(circle(30, 30, 17), p.hot, p.deep)}
${frame(rect(20, 27, 20, 6, 3), p.white, 2.5)}`,
    ),

    "stop-all": data(
      `${slab(circle(35, 23, 12), p.deep, p.main, 5, 5)}
${slab(circle(25, 35, 14), p.hot, p.deep, 5, 5)}
${detail("M19 29L31 41M31 29L19 41", p.ink, 8)}
${detail("M19 29L31 41M31 29L19 41", p.white, 4.5)}`,
    ),

    delete: data(
      `${slab("M18 22h28l-2.5 24a5 5 0 0 1-5 4.5H25.5a5 5 0 0 1-5-4.5Z", p.hot, p.deep)}
${slab(rect(15, 16, 34, 8, 4), p.deep, p.main, 4, 4)}
${slab(rect(27, 10, 10, 6, 3), p.deep, p.main, 3, 3)}
${detail("M26 31v12M32 31v12M38 31v12", p.ink, 2.5)}`,
    ),

    "delete-completed": data(
      `${slab(rect(10, 16, 44, 36, 8), p.deep, p.main)}
${detail("M12 26h40", p.ink, 3)}
${detail("M20 35L28 43L44 25", p.ink, 10)}
${detail("M20 35L28 43L44 25", p.white, 6)}`,
    ),

    options: data(
      `${slab(cog(45, 44, 11, 8), p.main, p.deep, 4, 4)}
${frame(circle(45, 44, 3.5), p.paper, 2.5)}
${slab(cog(24, 30, 16, 12), p.gold, p.hot)}
${frame(circle(24, 30, 5), p.paper, 2.8)}`,
    ),

    scheduler: data(
      `${detail("M22 46l-3 6M38 46l3 6", p.ink, 3)}
${slab(circle(17, 13, 5.5), p.hot, p.deep, 3, 3)}
${slab(circle(43, 13, 5.5), p.hot, p.deep, 3, 3)}
${slab(rect(26, 5, 8, 6, 3), p.hot, p.deep, 2, 2)}
${slab(circle(30, 31, 16), p.gold, p.hot)}
${frame(circle(30, 31, 11), p.paper, 2.8)}
${detail("M30 24v8l5 3", p.ink, 3.2)}`,
    ),

    "start-queue": data(
      `${slab(rect(28, 34, 7, 16, 3), p.gold, p.hot, 3, 3)}
${slab(rect(9, 12, 46, 26, 7), p.ok, p.deep)}
${frame(poly([[18, 19], [30, 25], [18, 31]]), p.white, 3)}
${detail("M36 20h11M36 26h11", p.white, 4)}`,
    ),

    "stop-queue": data(
      `${slab(rect(28, 34, 7, 16, 3), p.gold, p.hot, 3, 3)}
${slab(rect(9, 12, 46, 26, 7), p.hot, p.deep)}
${frame(rect(18, 19, 11, 11, 2), p.white, 2.5)}
${detail("M36 20h11M36 26h11", p.white, 4)}`,
    ),

    "tell-friend": data(
      `${slab(rect(9, 19, 46, 30, 6), p.paper, p.grey)}
${frame(poly([[11, 22], [32, 37], [53, 22]]), p.tint, 2.5)}
${slab("M46 24c-6-4-11-7-11-12 0-3 4-5 7-2l4 3 4-3c3-3 7-1 7 2 0 5-5 8-11 12Z", p.hot, p.deep, 3, 3)}`,
    ),
  };
}

export const TOOLBAR_ICONS_ISOMETRIC = paint(RISO);
export const TOOLBAR_ICONS_BLUEPRINT = paint(BLUEPRINT);
