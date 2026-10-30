/**
 * The Classic toolbar artwork.
 *
 * Flat fills, dark outlines and no gloss: the same folder, globe, gear and
 * clock motifs as the 3D Style set, drawn in the older desktop idiom. Every
 * drawing is original to this application.
 */

import { toolbarIconDataUrl } from "./toolbar-art.ts";
import type { ToolbarCommand } from "./toolbars.ts";

const OUTLINE = "#2f3a45";
const GREEN = "#3fae54";
const GREEN_DARK = "#1d6b31";
const RED = "#e05252";
const RED_DARK = "#8e2424";
const BLUE = "#4f8fce";
const BLUE_LIGHT = "#9ed0f5";
const YELLOW = "#f3c623";
const YELLOW_DARK = "#a57b00";
const STEEL = "#aab4be";
const PAPER = "#f7f9fb";
const ORANGE = "#ffcf3f";
const CREAM = "#fff6dc";

const GLOBE_CLIP = `<clipPath id="globeClip"><circle cx="13.5" cy="13.5" r="9.8"/></clipPath>`;

const GLOBE = `
<circle cx="13.5" cy="13.5" r="9.8" fill="${BLUE_LIGHT}" stroke="${OUTLINE}" stroke-width="1.6"/>
<g clip-path="url(#globeClip)">
  <path d="M5.5 9.2c2.5-1.6 4.8-.5 6.6 1.3s4.5 1.6 6.4-.5" fill="none" stroke="${GREEN}" stroke-width="2.6" stroke-linecap="round"/>
  <path d="M5.6 17.7c2.3 2 4.6 2.4 6.6.6s4.3-1.8 6.4.2" fill="none" stroke="${GREEN}" stroke-width="2.4" stroke-linecap="round"/>
</g>`;

function gear(x: number, y: number, scale: number): string {
  const teeth = [0, 45, 90, 135, 180, 225, 270, 315]
    .map(
      (angle) =>
        `<rect x="-2" y="-11" width="4" height="5" rx="1" transform="rotate(${angle})" fill="${STEEL}" stroke="${OUTLINE}" stroke-width="1"/>`,
    )
    .join("");
  return `<g transform="translate(${x} ${y}) scale(${scale})">${teeth}
<circle r="7" fill="${STEEL}" stroke="${OUTLINE}" stroke-width="1.2"/>
<circle r="2.7" fill="${OUTLINE}"/>
</g>`;
}

export const TOOLBAR_ICONS_CLASSIC: Record<ToolbarCommand, string> = {
  "add-download": toolbarIconDataUrl(
    `<path d="M5 20.5h22l-2.4 5.4a2 2 0 0 1-1.8 1.1H9.2a2 2 0 0 1-1.8-1.1Z" fill="${BLUE_LIGHT}" stroke="${OUTLINE}" stroke-width="1.6" stroke-linejoin="round"/>
<path d="M6.5 20.5 9 9.6a2 2 0 0 1 2-1.7h14.1a2 2 0 0 1 2 2.3l-1.5 10.3Z" fill="${YELLOW}" stroke="${OUTLINE}" stroke-width="1.6" stroke-linejoin="round"/>
<path d="M8.6 10.8h14.6" stroke="${YELLOW_DARK}" stroke-width="1.2" stroke-linecap="round"/>
<circle cx="24" cy="23.5" r="6.4" fill="${GREEN}" stroke="${OUTLINE}" stroke-width="1.5"/>
<path d="M24 20.4v6.2M20.9 23.5h6.2" stroke="#fff" stroke-width="2" stroke-linecap="round"/>`,
  ),

  resume: toolbarIconDataUrl(
    `${GLOBE}
<circle cx="23.5" cy="23.5" r="6.3" fill="${GREEN}" stroke="${OUTLINE}" stroke-width="1.5"/>
<path d="M21.1 20.9 26.2 23.5l-5.1 2.6z" fill="#fff" stroke="${GREEN_DARK}" stroke-width=".6" stroke-linejoin="round"/>`,
    GLOBE_CLIP,
  ),

  stop: toolbarIconDataUrl(
    `${GLOBE}
<path d="M20.4 17.2h7.2l3.6 3.6v7.2l-3.6 3.6h-7.2l-3.6-3.6v-7.2z" fill="${RED}" stroke="${OUTLINE}" stroke-width="1.5" stroke-linejoin="round"/>
<rect x="20.2" y="22.5" width="7.6" height="2.8" rx="1" fill="#fff"/>`,
    GLOBE_CLIP,
  ),

  "stop-all": toolbarIconDataUrl(
    `<rect x="4.5" y="5.5" width="23" height="23" rx="4.5" fill="${RED}" stroke="${OUTLINE}" stroke-width="1.7"/>
<path d="M10.6 11.6l10.8 10.8M21.4 11.6 10.6 22.4" stroke="#fff" stroke-width="2.8" stroke-linecap="round"/>`,
  ),

  delete: toolbarIconDataUrl(
    `<path d="M9.6 11h12.8l-1.3 14.1a2 2 0 0 1-2 1.8h-6.2a2 2 0 0 1-2-1.8Z" fill="${RED}" stroke="${OUTLINE}" stroke-width="1.6" stroke-linejoin="round"/>
<rect x="7.6" y="7.6" width="16.8" height="4" rx="1.7" fill="${RED}" stroke="${OUTLINE}" stroke-width="1.5"/>
<rect x="13" y="4.6" width="6" height="3" rx="1" fill="${RED}" stroke="${OUTLINE}" stroke-width="1.5"/>
<path d="M13.4 15.6v8.8M16 15.6v8.8M18.6 15.6v8.8" stroke="${RED_DARK}" stroke-width="1.4"/>`,
  ),

  "delete-completed": toolbarIconDataUrl(
    `<rect x="4.5" y="6.5" width="23" height="21" rx="4" fill="${BLUE}" stroke="${OUTLINE}" stroke-width="1.7"/>
<path d="M9 17.2l4.9 4.9 9.4-10.4" fill="none" stroke="#fff" stroke-width="3.1" stroke-linecap="round" stroke-linejoin="round"/>`,
  ),

  options: toolbarIconDataUrl(
    `${gear(23, 22, 0.66)}
${gear(12.5, 12.5, 1)}`,
  ),

  scheduler: toolbarIconDataUrl(
    `<path d="M10 28.5l-1.5 2.4M22 28.5l1.5 2.4" stroke="${OUTLINE}" stroke-width="1.8" stroke-linecap="round"/>
<circle cx="7.5" cy="7.5" r="3.2" fill="${ORANGE}" stroke="${OUTLINE}" stroke-width="1.4"/>
<circle cx="24.5" cy="7.5" r="3.2" fill="${ORANGE}" stroke="${OUTLINE}" stroke-width="1.4"/>
<path d="M13 4.5h6" stroke="${OUTLINE}" stroke-width="2" stroke-linecap="round"/>
<circle cx="16" cy="17" r="11" fill="${ORANGE}" stroke="${OUTLINE}" stroke-width="1.7"/>
<circle cx="16" cy="17" r="8.2" fill="${CREAM}" stroke="${OUTLINE}" stroke-width="1.1"/>
<path d="M16 11.5V17l3.7 2.2" fill="none" stroke="${OUTLINE}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
<circle cx="16" cy="17" r="1.1" fill="${OUTLINE}"/>`,
  ),

  "start-queue": toolbarIconDataUrl(
    `<path d="M16 24.5V29" stroke="${OUTLINE}" stroke-width="2.6" stroke-linecap="round"/>
<rect x="4.5" y="5.5" width="23" height="18.5" rx="3" fill="${GREEN}" stroke="${OUTLINE}" stroke-width="1.7"/>
<path d="M9 10.5l4.9 3-4.9 3z" fill="#fff" stroke="${GREEN_DARK}" stroke-width=".6" stroke-linejoin="round"/>
<path d="M17.5 10.5h6.5M17.5 14.5h6.5M9 19.5h15" stroke="#fff" stroke-width="1.7" stroke-linecap="round"/>`,
  ),

  "stop-queue": toolbarIconDataUrl(
    `<path d="M16 24.5V29" stroke="${OUTLINE}" stroke-width="2.6" stroke-linecap="round"/>
<rect x="4.5" y="5.5" width="23" height="18.5" rx="3" fill="${RED}" stroke="${OUTLINE}" stroke-width="1.7"/>
<rect x="8.7" y="10.3" width="6" height="6" rx="1" fill="#fff"/>
<path d="M17.5 10.5h6.5M17.5 14.5h6.5M9 19.5h15" stroke="#fff" stroke-width="1.7" stroke-linecap="round"/>`,
  ),

  "tell-friend": toolbarIconDataUrl(
    `<rect x="4.5" y="9.5" width="23" height="15" rx="2.4" fill="${PAPER}" stroke="${OUTLINE}" stroke-width="1.6"/>
<path d="M6 11.5 16 18.8 26 11.5" fill="none" stroke="${OUTLINE}" stroke-width="1.5" stroke-linejoin="round"/>
<circle cx="24.5" cy="8.5" r="6" fill="${GREEN}" stroke="${OUTLINE}" stroke-width="1.5"/>
<path d="M21.6 11.4 27.4 5.6M22.6 5.6h4.8v4.8" fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>`,
  ),
};
