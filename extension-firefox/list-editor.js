/*
  A small editable list: add, remove, drag to reorder, and Up/Down on a handle
  for people who are not using a pointer. Used by the settings page for the
  file types and the sites left alone.

  The order is the order the rows are shown in; the takeover rule is a set
  lookup, so moving a row changes nothing about which file is taken.
*/

const GRIP =
  "M3 2.5h1.5v1.5H3zM7 2.5h1.5v1.5H7zM3 6.25h1.5v1.5H3zM7 6.25h1.5v1.5H7zM3 10h1.5v1.5H3zM7 10h1.5v1.5H7z";
const CROSS = "M3 3l6 6M9 3l-6 6";

function createListEditor(root, { clean, onChange, onMove }) {
  const list = root.querySelector(".list");
  const empty = root.querySelector(".empty");
  const input = root.querySelector(".add-input");
  const addButton = root.querySelector(".add-button");

  let dragged = null;

  const values = () => [...list.querySelectorAll(".list-row")].map((row) => row.dataset.value);

  function rowFor(value) {
    const row = document.createElement("li");
    row.className = "list-row";
    row.draggable = true;
    row.dataset.value = value;

    const handle = document.createElement("button");
    handle.type = "button";
    handle.className = "handle";
    handle.setAttribute("aria-label", `Move ${value}`);
    handle.title = "Press Up or Down to move this row";
    handle.append(icon(GRIP, 12, 12));

    const label = document.createElement("span");
    label.className = "value";
    label.textContent = value;

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "remove";
    remove.setAttribute("aria-label", `Remove ${value}`);
    remove.append(icon(CROSS, 12, 12));
    remove.addEventListener("click", () => {
      row.remove();
      sync();
      onChange();
    });

    handle.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
      event.preventDefault();
      move(row, event.key === "ArrowUp" ? -1 : 1);
    });

    row.addEventListener("dragstart", (event) => {
      dragged = row;
      row.classList.add("dragging");
      event.dataTransfer.effectAllowed = "move";
      // Firefox needs data on the transfer or the drag never starts.
      event.dataTransfer.setData("text/plain", value);
    });

    row.addEventListener("dragend", () => {
      row.classList.remove("dragging");
      dragged = null;
      onChange();
    });

    row.append(handle, label, remove);
    return row;
  }

  function icon(path, width, height) {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
    svg.setAttribute("width", "12");
    svg.setAttribute("height", "12");
    svg.setAttribute("aria-hidden", "true");
    const shape = document.createElementNS("http://www.w3.org/2000/svg", "path");
    shape.setAttribute("d", path);
    shape.setAttribute("fill", "currentColor");
    svg.append(shape);
    return svg;
  }

  function announce(row) {
    if (!onMove) return;
    onMove(row.dataset.value, [...list.children].indexOf(row), list.children.length);
  }

  function move(row, delta) {
    const rows = [...list.children];
    const index = rows.indexOf(row);
    const target = index + delta;
    if (target < 0 || target >= rows.length) return;

    if (delta < 0) list.insertBefore(row, rows[target]);
    else list.insertBefore(row, rows[target].nextSibling);

    // The row moved, so focus would be lost without this.
    row.querySelector(".handle").focus();
    announce(row);
    onChange();
  }

  function sync() {
    empty.hidden = list.children.length > 0;
  }

  function add() {
    const additions = input.value
      .split(/[\s,]+/)
      .map(clean)
      .filter(Boolean);

    let added = false;
    for (const value of additions) {
      if (values().includes(value)) continue;
      list.append(rowFor(value));
      added = true;
    }

    input.value = "";
    sync();
    if (added) onChange();
    input.focus();
  }

  list.addEventListener("dragover", (event) => {
    if (!dragged) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";

    const after = [...list.querySelectorAll(".list-row:not(.dragging)")].find((row) => {
      const box = row.getBoundingClientRect();
      return event.clientY < box.top + box.height / 2;
    });

    if (after) list.insertBefore(dragged, after);
    else list.append(dragged);
  });

  list.addEventListener("drop", (event) => {
    if (!dragged) return;
    event.preventDefault();
    announce(dragged);
  });

  addButton.addEventListener("click", add);
  input.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    add();
  });

  return {
    values,
    set(nextValues) {
      list.textContent = "";
      for (const value of nextValues) list.append(rowFor(value));
      sync();
    },
    focus() {
      input.focus();
    },
  };
}
