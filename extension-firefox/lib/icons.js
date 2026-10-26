/*
  The icons the extension draws, built as SVG elements rather than markup.

  Assigning markup to `innerHTML` is what web-ext warns about, even when the
  string is a constant, and a DOM node is also easier to reuse: every icon
  takes the colour and the size of the control it sits in.
*/

const BDM_SVG_NS = "http://www.w3.org/2000/svg";

const BDM_ICON_SHAPES = {
  download: {
    paths: ["M12 3v11", "m7 9 5 5 5-5", "M5 21h14"],
  },
  check: {
    paths: ["m5 12 5 5L20 6"],
  },
  close: {
    paths: ["M18 6 6 18", "m6 6 12 12"],
  },
  pause: {
    paths: ["M9 5v14", "M15 5v14"],
  },
  play: {
    paths: ["m8 5 11 7-11 7V5Z"],
  },
  sliders: {
    paths: ["M4 7h9", "M17 7h3", "M4 17h3", "M11 17h9"],
    circles: [
      { cx: 15, cy: 7, r: 2 },
      { cx: 9, cy: 17, r: 2 },
    ],
  },
  refresh: {
    paths: ["M20 11a8 8 0 1 0-2.3 5.7", "M20 5v6h-6"],
  },
  list: {
    paths: ["M9 6h11", "M9 12h11", "M9 18h11", "M4 6h.01", "M4 12h.01", "M4 18h.01"],
  },
};

/** One icon, tinted and sized by the CSS of its control. */
function bdmIcon(name, className) {
  const shape = BDM_ICON_SHAPES[name] || BDM_ICON_SHAPES.download;
  const svg = document.createElementNS(BDM_SVG_NS, "svg");

  if (className) svg.setAttribute("class", className);
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.9");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");

  for (const path of shape.paths || []) {
    const element = document.createElementNS(BDM_SVG_NS, "path");
    element.setAttribute("d", path);
    svg.append(element);
  }

  for (const circle of shape.circles || []) {
    const element = document.createElementNS(BDM_SVG_NS, "circle");
    element.setAttribute("cx", String(circle.cx));
    element.setAttribute("cy", String(circle.cy));
    element.setAttribute("r", String(circle.r));
    svg.append(element);
  }

  return svg;
}
