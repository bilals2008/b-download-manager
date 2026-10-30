/**
 * The 3D Style toolbar artwork.
 *
 * Original vector drawings in the visual language of classic Windows download
 * managers: glossy folders, globes, beveled boxes, steel gears, an alarm clock
 * and a sign board. They are drawn here, not lifted from another product; the
 * only borrowed thing is the motif vocabulary.
 */

import { toolbarIconDataUrl } from "./toolbar-art.ts";
import type { ToolbarCommand } from "./toolbars.ts";

const GLOBE_DEFS = `
<radialGradient id="globe" cx=".34" cy=".28" r=".85">
  <stop offset="0" stop-color="#d8f2ff"/>
  <stop offset=".55" stop-color="#5aabea"/>
  <stop offset="1" stop-color="#14558f"/>
</radialGradient>
<clipPath id="globeClip"><circle cx="13.5" cy="13.5" r="9.6"/></clipPath>`;

const GLOBE = `
<ellipse cx="16" cy="29" rx="10.5" ry="2" fill="#000" opacity=".16"/>
<circle cx="13.5" cy="13.5" r="9.6" fill="url(#globe)" stroke="#0d4a7a" stroke-width="1"/>
<g clip-path="url(#globeClip)">
  <path d="M6.4 9.4c2.4-1.7 4.7-.6 6.5 1.2s4.4 1.6 6.3-.4" fill="none" stroke="#3aa64c" stroke-width="2.6" stroke-linecap="round" opacity=".9"/>
  <path d="M6.4 17.6c2.2 2 4.5 2.4 6.5.6s4.2-1.8 6.3.2" fill="none" stroke="#3aa64c" stroke-width="2.4" stroke-linecap="round" opacity=".85"/>
</g>
<ellipse cx="9.8" cy="8.8" rx="2.8" ry="1.8" fill="#fff" opacity=".45" transform="rotate(-28 9.8 8.8)"/>`;

const RED_DEFS = `<linearGradient id="red" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ef6b5e"/><stop offset=".55" stop-color="#d64545"/><stop offset="1" stop-color="#9b2626"/></linearGradient>`;
const GREEN_DEFS = `<linearGradient id="green" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#63da6b"/><stop offset=".55" stop-color="#2f9e44"/><stop offset="1" stop-color="#146c2e"/></linearGradient>`;
const BLUE_DEFS = `<linearGradient id="blue" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#79bdf3"/><stop offset=".55" stop-color="#2f6fb3"/><stop offset="1" stop-color="#174a7c"/></linearGradient>`;
const STEEL_DEFS = `<linearGradient id="steel" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#e1e7ec"/><stop offset=".5" stop-color="#9aa7b3"/><stop offset="1" stop-color="#59646e"/></linearGradient>`;
const ORANGE_DEFS = `<linearGradient id="bell" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffd43b"/><stop offset="1" stop-color="#f08c00"/></linearGradient>`;
const PAPER_DEFS = `<linearGradient id="paper" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#e8edf2"/></linearGradient>`;

const SHADOW = `<ellipse cx="16" cy="29" rx="10.5" ry="2" fill="#000" opacity=".16"/>`;

function gear(x: number, y: number, scale: number): string {
  const teeth = [0, 45, 90, 135, 180, 225, 270, 315]
    .map(
      (angle) =>
        `<rect x="-2" y="-11" width="4" height="5" rx="1" transform="rotate(${angle})" fill="url(#steel)" stroke="#46525c" stroke-width=".8"/>`,
    )
    .join("");
  return `<g transform="translate(${x} ${y}) scale(${scale})">${teeth}
<circle r="7" fill="url(#steel)" stroke="#46525c" stroke-width="1"/>
<circle r="2.7" fill="#26313a"/>
<path d="M-5 -3.6A6 6 0 0 1 1.2 -6.6" fill="none" stroke="#fff" stroke-opacity=".5" stroke-width="1.3" stroke-linecap="round"/>
</g>`;
}

export const TOOLBAR_ICONS_3D: Record<ToolbarCommand, string> = {
  "add-download": toolbarIconDataUrl(
    `<ellipse cx="16" cy="29" rx="11" ry="2.1" fill="#000" opacity=".16"/>
<path d="M5 20.5h22l-2.4 5.4a2 2 0 0 1-1.8 1.1H9.2a2 2 0 0 1-1.8-1.1Z" fill="url(#tray)" stroke="#1d4f80" stroke-width="1"/>
<path d="M6.5 20.5 9 9.6a2 2 0 0 1 2-1.7h14.1a2 2 0 0 1 2 2.3l-1.5 10.3Z" fill="url(#folder)" stroke="#a97400" stroke-width="1"/>
<path d="M8.6 10.5h14.6" stroke="#fff" stroke-opacity=".55" stroke-width="1.2" stroke-linecap="round"/>
<circle cx="24" cy="23.5" r="6.4" fill="url(#plus)" stroke="#116b27" stroke-width="1"/>
<path d="M24 20.4v6.2M20.9 23.5h6.2" stroke="#fff" stroke-width="2" stroke-linecap="round"/>`,
    `<linearGradient id="folder" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffe873"/><stop offset="1" stop-color="#f0b400"/></linearGradient>
<linearGradient id="tray" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#86b9ea"/><stop offset="1" stop-color="#2f6fb3"/></linearGradient>
<linearGradient id="plus" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#5cd663"/><stop offset="1" stop-color="#1e8f36"/></linearGradient>`,
  ),

  resume: toolbarIconDataUrl(
    `${GLOBE}
<circle cx="23.5" cy="23.5" r="6.2" fill="url(#green)" stroke="#0d6b28" stroke-width="1"/>
<path d="M21.1 20.9 26.2 23.5l-5.1 2.6z" fill="#fff" stroke="#0d6b28" stroke-width=".5" stroke-linejoin="round"/>`,
    GLOBE_DEFS + GREEN_DEFS,
  ),

  stop: toolbarIconDataUrl(
    `${GLOBE}
<path d="M20.4 17.2h7.2l3.6 3.6v7.2l-3.6 3.6h-7.2l-3.6-3.6v-7.2z" fill="url(#red)" stroke="#7d1d18" stroke-width="1"/>
<path d="M21 19.2h6" stroke="#ffb3a8" stroke-opacity=".65" stroke-width="1"/>
<rect x="20.2" y="22.5" width="7.6" height="2.8" rx="1" fill="#fff"/>`,
    GLOBE_DEFS + RED_DEFS,
  ),

  "stop-all": toolbarIconDataUrl(
    `${SHADOW}
<rect x="4.5" y="5.5" width="23" height="23" rx="4.5" fill="url(#red)" stroke="#7d1d18" stroke-width="1.1"/>
<path d="M5.5 9.2h21" stroke="#ffb3a8" stroke-opacity=".65" stroke-width="1.3" stroke-linecap="round"/>
<path d="M5 24.4h22" stroke="#5f120e" stroke-opacity=".5" stroke-width="1.4"/>
<path d="M10.6 11.6l10.8 10.8M21.4 11.6 10.6 22.4" stroke="#fff" stroke-width="2.6" stroke-linecap="round"/>`,
    RED_DEFS,
  ),

  delete: toolbarIconDataUrl(
    `${SHADOW}
<path d="M9.6 11h12.8l-1.3 14.1a2 2 0 0 1-2 1.8h-6.2a2 2 0 0 1-2-1.8Z" fill="url(#red)" stroke="#7d1d18" stroke-width="1.1"/>
<path d="M11.2 13h9.6" stroke="#ffb3a8" stroke-opacity=".55" stroke-width="1.2" stroke-linecap="round"/>
<rect x="7.6" y="7.6" width="16.8" height="4" rx="1.7" fill="url(#red)" stroke="#7d1d18" stroke-width="1"/>
<rect x="13" y="4.6" width="6" height="3" rx="1" fill="url(#red)" stroke="#7d1d18" stroke-width="1"/>
<path d="M13.4 15.6v8.8M16 15.6v8.8M18.6 15.6v8.8" stroke="#fff" stroke-opacity=".45" stroke-width="1.2"/>`,
    RED_DEFS,
  ),

  "delete-completed": toolbarIconDataUrl(
    `${SHADOW}
<rect x="4.5" y="6.5" width="23" height="21" rx="4" fill="url(#blue)" stroke="#174a7c" stroke-width="1.1"/>
<path d="M5.5 10h21" stroke="#bfe3ff" stroke-opacity=".6" stroke-width="1.3" stroke-linecap="round"/>
<path d="M9 17.2l4.9 4.9 9.4-10.4" fill="none" stroke="#fff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`,
    BLUE_DEFS,
  ),

  options: toolbarIconDataUrl(
    `${SHADOW}
${gear(23, 22, 0.66)}
${gear(12.5, 12.5, 1)}`,
    STEEL_DEFS,
  ),

  scheduler: toolbarIconDataUrl(
    `${SHADOW}
<path d="M10 28.5l-1.5 2.4M22 28.5l1.5 2.4" stroke="#8a4b00" stroke-width="1.8" stroke-linecap="round"/>
<circle cx="7.5" cy="7.5" r="3.2" fill="url(#bell)" stroke="#8a4b00" stroke-width="1"/>
<circle cx="24.5" cy="7.5" r="3.2" fill="url(#bell)" stroke="#8a4b00" stroke-width="1"/>
<path d="M13 4.5h6" stroke="#8a4b00" stroke-width="2" stroke-linecap="round"/>
<circle cx="16" cy="17" r="11" fill="url(#bell)" stroke="#8a4b00" stroke-width="1"/>
<path d="M7 12.2c1.5-4 5-6.2 9-6.2" fill="none" stroke="#fff" stroke-opacity=".5" stroke-width="1.5" stroke-linecap="round"/>
<circle cx="16" cy="17" r="8.2" fill="#fff8e8" stroke="#c47a00" stroke-width=".9"/>
<path d="M16 11.5V17l3.7 2.2" fill="none" stroke="#31353a" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/>
<circle cx="16" cy="17" r="1.1" fill="#31353a"/>`,
    ORANGE_DEFS,
  ),

  "start-queue": toolbarIconDataUrl(
    `${SHADOW}
<path d="M16 24.5V29" stroke="#6b4a12" stroke-width="2.6" stroke-linecap="round"/>
<rect x="4.5" y="5.5" width="23" height="18.5" rx="3" fill="url(#green)" stroke="#0b5f22" stroke-width="1"/>
<path d="M6.2 8.2h19.6" stroke="#d8ffdd" stroke-opacity=".6" stroke-width="1.3" stroke-linecap="round"/>
<path d="M9 10.5l4.9 3-4.9 3z" fill="#fff" stroke="#0b5f22" stroke-width=".5" stroke-linejoin="round"/>
<path d="M17.5 10.5h6.5M17.5 14.5h6.5M9 19.5h15" stroke="#fff" stroke-opacity=".85" stroke-width="1.6" stroke-linecap="round"/>`,
    GREEN_DEFS,
  ),

  "stop-queue": toolbarIconDataUrl(
    `${SHADOW}
<path d="M16 24.5V29" stroke="#6b4a12" stroke-width="2.6" stroke-linecap="round"/>
<rect x="4.5" y="5.5" width="23" height="18.5" rx="3" fill="url(#red)" stroke="#7d1d18" stroke-width="1"/>
<path d="M6.2 8.2h19.6" stroke="#ffc4bb" stroke-opacity=".6" stroke-width="1.3" stroke-linecap="round"/>
<rect x="8.7" y="10.3" width="6" height="6" rx="1" fill="#fff"/>
<path d="M17.5 10.5h6.5M17.5 14.5h6.5M9 19.5h15" stroke="#fff" stroke-opacity=".85" stroke-width="1.6" stroke-linecap="round"/>`,
    RED_DEFS,
  ),

  "tell-friend": toolbarIconDataUrl(
    `${SHADOW}
<rect x="4.5" y="9.5" width="23" height="15" rx="2.4" fill="url(#paper)" stroke="#8a6a00" stroke-width="1"/>
<path d="M6 11.5 16 18.8 26 11.5" fill="none" stroke="#b08a11" stroke-width="1.6" stroke-linejoin="round"/>
<circle cx="24.5" cy="8.5" r="6" fill="url(#green)" stroke="#0b6b22" stroke-width="1"/>
<path d="M21.6 11.4 27.4 5.6M22.6 5.6h4.8v4.8" fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>`,
    PAPER_DEFS + GREEN_DEFS,
  ),
};
