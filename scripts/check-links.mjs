// Behaviour checks for the Download all links window.
//
// The window is an extension page, so it can be run outside Firefox against a
// small document: the controls it names, a select's options, a checkbox and a
// click. The checks cover what the window is for — the list and its filters,
// the page-link switch, selecting, and the chunked hand-over with one answer
// per link — rather than the browser's rendering.
//
// Loaded from scripts/check-extension.mjs, so `npm run check:extension` runs
// them with the rest.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import vm from "node:vm";

const rootDir = fileURLToPath(new URL("../extension-firefox/", import.meta.url));
const read = (file) => readFileSync(path.join(rootDir, file), "utf8");

/* ------------------------------------------------------------ the document */

/** One element, with only the parts the window uses. */
function createElement(tag) {
  const element = {
    tagName: String(tag).toUpperCase(),
    children: [],
    parentNode: null,
    className: "",
    id: "",
    dataset: {},
    value: "",
    checked: false,
    disabled: false,
    hidden: false,
    title: "",
    listeners: new Map(),
    attributes: new Map(),
    _text: "",
    options: [],

    setAttribute(name, value) {
      this.attributes.set(name, String(value));
      if (name === "class") this.className = String(value);
    },

    getAttribute(name) {
      return this.attributes.has(name) ? this.attributes.get(name) : null;
    },

    append(...nodes) {
      for (const node of nodes) {
        if (node && node.__fragment) {
          this.append(...node.children);
          continue;
        }
        if (!node || typeof node !== "object") continue;
        node.parentNode = this;
        this.children.push(node);
        if (this.tagName === "SELECT" && node.__option) {
          this.options.push(node);
          if (this.value === "" && this.options.length === 1) {
            this.value = node.value;
          }
        }
      }
    },

    get textContent() {
      if (this._text) return this._text;
      return this.children.map((child) => child.textContent || "").join("");
    },

    set textContent(value) {
      this.children = [];
      this.options = [];
      this._text = String(value);
    },

    get selectedOptions() {
      const chosen = this.options.find((option) => option.value === this.value);
      return chosen ? [chosen] : [];
    },

    addEventListener(type, listener) {
      if (!this.listeners.has(type)) this.listeners.set(type, []);
      this.listeners.get(type).push(listener);
    },

    removeEventListener(type, listener) {
      const entries = this.listeners.get(type) || [];
      this.listeners.set(
        type,
        entries.filter((entry) => entry !== listener),
      );
    },
  };
  return element;
}

/** A click or a change, delivered to the listeners of one element. */
function fire(element, type) {
  for (const listener of element.listeners.get(type) || []) {
    listener({ type, target: element, preventDefault() {} });
  }
}

class Option {
  constructor(text, value) {
    this.__option = true;
    this.textContent = String(text);
    this.value = String(value);
    this.title = "";
  }
}

function createDocument(ids) {
  const elements = new Map();
  const document = {
    title: "",
    documentElement: { dataset: {} },
    listeners: new Map(),
    getElementById(id) {
      return elements.get(id);
    },
    createElement,
    createDocumentFragment() {
      return {
        __fragment: true,
        children: [],
        append(...nodes) {
          this.children.push(...nodes);
        },
      };
    },
    addEventListener(type, listener) {
      if (!document.listeners.has(type)) document.listeners.set(type, []);
      document.listeners.get(type).push(listener);
    },
  };

  // A select is a select and a checkbox is a checkbox. The ids are the ones
  // `scripts/check-extension.mjs` also checks against `links.html`.
  const kinds = {
    category: "select",
    extension: "select",
    folder: "select",
    queue: "select",
    connections: "input",
    pages: "input",
    links: "ul",
    "report-list": "ul",
  };
  for (const id of ids) {
    const element = createElement(kinds[id] || "div");
    element.id = id;
    elements.set(id, element);
  }
  elements.get("connections").value = "8";
  elements.get("queue").value = "now";

  return { document, elements };
}

/* ------------------------------------------------------------- the window */

const ids = [
  "state",
  "state-text",
  "retry",
  "page-name",
  "close",
  "category",
  "extension",
  "pages-row",
  "pages",
  "pages-count",
  "select-shown",
  "clear-all",
  "links",
  "empty",
  "report",
  "report-title",
  "report-list",
  "folder",
  "queue",
  "connections",
  "selection",
  "send",
  "cancel",
  "note",
];

const { document, elements } = createDocument(ids);

/** The links the page reader would answer with: twelve files, one a song. */
const files = Array.from({ length: 11 }, (_value, index) => ({
  url: `https://example.org/files/f${index + 1}.mp4`,
  key: `https://example.org/files/f${index + 1}.mp4`,
  name: `f${index + 1}.mp4`,
  extension: "mp4",
  category: "Video",
  kind: "file",
  badge: "MP4",
}));

const song = {
  url: "https://example.org/files/song.mp3",
  key: "https://example.org/files/song.mp3",
  name: "song.mp3",
  extension: "mp3",
  category: "Music",
  kind: "file",
  badge: "MP3",
};

const pageLink = {
  url: "https://example.org/about",
  key: "https://example.org/about",
  name: "about",
  extension: "",
  category: "Other",
  kind: "page",
  badge: "PAGE",
};

const scanAnswer = {
  ok: true,
  page: {
    url: "https://example.org/files/",
    host: "example.org",
    title: "A page of files",
  },
  links: [...files, song, pageLink],
  stats: {
    scanned: 40,
    total: 40,
    found: files.length + 2,
    files: files.length + 1,
    pages: 1,
    duplicates: 2,
    skipped: { stream: 1, scheme: 0, self: 1, unusable: 0 },
    truncated: false,
  },
};

const calls = { batches: [], scans: 0, closed: false };
const optionsAnswer = {
  ok: true,
  folders: [
    { label: "App default", path: "C:\\Users\\me\\Downloads" },
    { label: "Last used", path: "C:\\Users\\me\\Downloads\\Films" },
  ],
  queues: [
    { id: "now", label: "Start now" },
    { id: "main", label: "Main queue" },
  ],
  connections: 8,
  maxItems: 25,
};

/** One answer per link, with the third link of the whole batch refused. */
let answered = 0;
function batchAnswer(body) {
  const items = body.items.map((item) => {
    answered += 1;
    if (answered === 3) {
      return {
        url: item.url,
        name: item.name,
        status: "refused",
        id: null,
        dest: "",
        message: "The server refused the link.",
      };
    }
    return {
      url: item.url,
      name: item.name,
      status: "started",
      id: 100 + answered,
      dest: `C:\\Users\\me\\Downloads\\Films\\${item.name}`,
      message: "",
    };
  });
  const refused = items.filter((item) => item.status === "refused").length;
  return {
    ok: true,
    result: {
      folder: body.folder || "C:\\Users\\me\\Downloads",
      queue: body.queue,
      connections: body.connections,
      counts: {
        started: items.length - refused,
        queued: 0,
        skipped: 0,
        duplicates: 0,
        invalid: 0,
        refused,
      },
      items,
    },
  };
}

const context = vm.createContext({
  URL,
  URLSearchParams,
  Option,
  console,
  document,
  crypto: { randomUUID: () => "scan-test" },
  window: {
    location: { search: "?tab=5" },
    close: () => {
      calls.closed = true;
    },
  },
  browser: {
    runtime: {
      sendMessage: async (message) => {
        if (message.type === "bdm:links-scan") {
          calls.scans += 1;
          assert.equal(message.tabId, 5, "the window reads its own tab");
          return scanAnswer;
        }
        if (message.type === "bdm:batch-options") return optionsAnswer;
        if (message.type === "bdm:batch") {
          calls.batches.push(message.payload);
          return batchAnswer(message.payload);
        }
        return { ok: true };
      },
      onMessage: { addListener: () => {} },
    },
    storage: {
      local: { get: async (defaults) => ({ ...defaults }) },
      onChanged: { addListener: () => {} },
    },
  },
});

for (const file of ["lib/settings.js", "lib/link-rules.js", "links/links.js"]) {
  vm.runInContext(read(file), context, { filename: file });
}

/** Let the requests the window makes settle. */
async function settle(times = 10) {
  for (let index = 0; index < times; index += 1) {
    await new Promise((resolve) => setTimeout(resolve, 2));
    await Promise.resolve();
  }
}
await settle();

/* --------------------------------------------------------------- the list */

const rows = () => elements.get("links").children;
const footer = () => elements.get("selection").textContent;
const send = () => elements.get("send");
const stateLine = () => elements.get("state-text").textContent;

assert.equal(calls.scans, 1, "the page is read once");
assert.equal(elements.get("page-name").textContent, "A page of files");
assert.equal(rows().length, 12, "the files are listed and the page link is not");
assert.equal(footer(), "12 links selected", "the files are ticked by default");
assert.equal(send().textContent, "Download 12 links");
assert.equal(send().disabled, false, "a running app makes the batch possible");
assert.equal(
  rows()[0].children[0].children[0].getAttribute("aria-label"),
  "f1.mp4",
  "a row's checkbox is named by its file",
);
assert.match(stateLine(), /12 files · 1 page link · 4 left out/);

// The scan's report names what was left out, before anything is sent.
assert.equal(elements.get("report").hidden, false);
assert.match(elements.get("report-title").textContent, /left out/);
const scanReport = elements.get("report-list").children;
assert.ok(
  scanReport.some((row) => row.textContent.includes("stream manifests")),
  "an unsupported stream is reported",
);
assert.ok(
  scanReport.some((row) => row.textContent.includes("same address")),
  "a duplicate address is reported",
);

// A page link joins the list only when the switch is on, and it starts unticked.
elements.get("pages").checked = true;
fire(elements.get("pages"), "change");
assert.equal(rows().length, 13, "the page link joins the list");
const pageRow = rows().find((row) => row.dataset.kind === "page");
assert.ok(pageRow, "the page row is marked as a page");
assert.equal(pageRow.dataset.checked, "false", "a page is not ticked for the user");
assert.equal(pageRow.children[0].children[1].textContent, "PAGE");
assert.equal(footer(), "12 links selected");

// A category filter narrows the list without touching the selection.
elements.get("category").value = "Music";
fire(elements.get("category"), "change");
assert.equal(rows().length, 1, "the category filter shows one type");
assert.equal(rows()[0].dataset.key, "https://example.org/files/song.mp3");
assert.equal(footer(), "12 links selected", "filtering does not change the selection");

elements.get("category").value = "";
fire(elements.get("category"), "change");
assert.equal(rows().length, 13);

// A type filter narrows it too.
elements.get("extension").value = "mp4";
fire(elements.get("extension"), "change");
assert.equal(rows().length, 11, "the type filter shows the mp4 files");
elements.get("extension").value = "";
fire(elements.get("extension"), "change");
assert.equal(rows().length, 13);

// Clearing and selecting the shown links works on the visible ones.
fire(elements.get("clear-all"), "click");
assert.equal(footer(), "0 links selected");
assert.equal(send().disabled, true, "an empty selection has nothing to send");
fire(elements.get("select-shown"), "click");
assert.equal(footer(), "13 links selected", "select shown takes the page link too");

/* --------------------------------------------------------------- the send */

elements.get("folder").value = "C:\\Users\\me\\Downloads\\Films";
elements.get("queue").value = "main";
elements.get("connections").value = "4";
fire(send(), "click");
await settle(24);

assert.equal(calls.batches.length, 2, "thirteen links go in two chunks");
assert.equal(calls.batches[0].items.length, 10);
assert.equal(calls.batches[1].items.length, 3);
assert.equal(calls.batches[0].folder, "C:\\Users\\me\\Downloads\\Films");
assert.equal(calls.batches[0].queue, "main");
assert.equal(calls.batches[0].connections, 4);
assert.equal(calls.batches[0].referer, "https://example.org/files/");
assert.ok(
  calls.batches[0].items.every((item) =>
    ["Video", "Music", "Other"].includes(item.category),
  ),
  "the app gets a category it knows",
);

assert.equal(send().disabled, true, "a sent batch cannot be sent twice");
assert.equal(footer(), "0 links selected · 13 sent");
assert.match(stateLine(), /12 started · 1 refused/);
assert.equal(elements.get("report-title").textContent, "What did not start");
const refusedReport = elements.get("report-list").children;
assert.equal(refusedReport.length, 1);
assert.match(refusedReport[0].textContent, /f3\.mp4/);
assert.match(refusedReport[0].textContent, /The server refused the link/);

const sentRows = rows().filter((row) => row.dataset.sent === "true");
assert.equal(sentRows.length, 13, "every answered link is marked");
assert.ok(
  sentRows.every((row) => row.children[0].children[0].disabled === true),
  "an answered link cannot be sent again",
);

console.log("links: all checks pass");
