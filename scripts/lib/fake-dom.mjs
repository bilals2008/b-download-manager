/*
  A small page for the extension checks.

  The content script is plain JavaScript against a DOM, so it can be run
  outside a browser if the pieces it touches exist: elements, a shadow root,
  event propagation with a composed path, mutations, sizes and a pointer. That
  is what this module builds. It is deliberately small — only what the overlay,
  the registry and the page reader call — but it keeps the rules that mattered
  in the browser: a closed shadow root is not visible in an event path, a
  pointer event has client coordinates, and a removed node is disconnected.

  This is a test harness, not a DOM implementation: selectors are the simple
  kinds the extension uses, and nothing is parsed from HTML.
*/

/* --------------------------------------------------------------- selectors */

function classNamesOf(element) {
  return String(element.className || "")
    .split(/\s+/)
    .filter(Boolean);
}

function matchesSimple(element, selector) {
  if (element.nodeType !== 1) return false;
  let rest = selector;

  const tag = /^([a-zA-Z][\w-]*|\*)/.exec(rest);
  if (tag) {
    if (tag[1] !== "*" && element.localName !== tag[1].toLowerCase()) return false;
    rest = rest.slice(tag[0].length);
  }

  while (rest.length) {
    if (rest[0] === ".") {
      const match = /^\.([\w-]+)/.exec(rest);
      if (!match || !classNamesOf(element).includes(match[1])) return false;
      rest = rest.slice(match[0].length);
    } else if (rest[0] === "#") {
      const match = /^#([\w-]+)/.exec(rest);
      if (!match || element.id !== match[1]) return false;
      rest = rest.slice(match[0].length);
    } else if (rest[0] === "[") {
      const match = /^\[([\w-]+)(?:="([^"]*)")?\]/.exec(rest);
      if (!match) return false;
      const value = element.getAttribute(match[1]);
      if (value === null) return false;
      if (match[2] !== undefined && value !== match[2]) return false;
      rest = rest.slice(match[0].length);
    } else {
      return false;
    }
  }
  return true;
}

function matches(element, selector) {
  return String(selector)
    .split(",")
    .some((part) => matchesSimple(element, part.trim()));
}

function camelToDash(name) {
  return String(name).replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
}

/* ------------------------------------------------------------------ nodes */

class FakeNode {
  constructor(ownerDocument, nodeType, localName) {
    this.ownerDocument = ownerDocument;
    this.nodeType = nodeType;
    this.localName = localName;
    this.children = [];
    this.parentNode = null;
    this.attributes = new Map();
    // `dataset` is a view of the attributes, as it is in a page: the overlay's
    // stylesheet selects on `:host([data-visible="true"])`.
    this.dataset = new Proxy(
      {},
      {
        get: (_target, key) => {
          if (typeof key !== "string") return undefined;
          const value = this.getAttribute(`data-${camelToDash(key)}`);
          return value === null ? undefined : value;
        },
        set: (_target, key, value) => {
          this.setAttribute(`data-${camelToDash(String(key))}`, value);
          return true;
        },
        has: (_target, key) => this.hasAttribute(`data-${camelToDash(String(key))}`),
      },
    );
    this.style = { cssText: "" };
    this.className = "";
    this.listeners = new Map();
    this.shadowRootClosed = null;
  }

  get tagName() {
    return String(this.localName || "").toUpperCase();
  }

  get isConnected() {
    let node = this;
    while (node.parentNode) node = node.parentNode;
    return node === this.ownerDocument.documentElement || node === this.ownerDocument;
  }

  /* ------------------------------------------------------------ children */

  appendChild(child) {
    return this.insertBefore(child, null);
  }

  insertBefore(child, before) {
    if (child.parentNode) child.parentNode.removeChildren([child]);
    child.parentNode = this;
    const index = before ? this.children.indexOf(before) : -1;
    if (index >= 0) this.children.splice(index, 0, child);
    else this.children.push(child);
    this.ownerDocument.recordMutation(this, [child], []);
    return child;
  }

  append(...nodes) {
    for (const node of nodes) {
      this.appendChild(typeof node === "string" ? this.ownerDocument.createTextNode(node) : node);
    }
  }

  replaceChildren(...nodes) {
    const removed = this.children.slice();
    this.children = [];
    for (const child of removed) child.parentNode = null;
    this.ownerDocument.recordMutation(this, [], removed);
    this.append(...nodes);
  }

  removeChildren(nodes) {
    this.children = this.children.filter((child) => !nodes.includes(child));
    for (const child of nodes) child.parentNode = null;
    this.ownerDocument.recordMutation(this, [], nodes);
  }

  remove() {
    this.parentNode?.removeChildren([this]);
  }

  get firstChild() {
    return this.children[0] || null;
  }

  get previousSibling() {
    const siblings = this.parentNode?.children || [];
    const index = siblings.indexOf(this);
    return index > 0 ? siblings[index - 1] : null;
  }

  /* --------------------------------------------------------------- text */

  get textContent() {
    if (this.nodeType === 3) return this.data || "";
    return this.children.map((child) => child.textContent).join("");
  }

  set textContent(value) {
    if (this.nodeType === 3) {
      this.data = String(value);
      return;
    }
    const removed = this.children.slice();
    this.children = [];
    for (const child of removed) child.parentNode = null;
    if (removed.length) this.ownerDocument.recordMutation(this, [], removed);
    if (value !== "") this.append(String(value));
  }

  /* --------------------------------------------------------- attributes */

  setAttribute(name, value) {
    this.attributes.set(name, String(value));
    if (name === "class") this.className = String(value);
  }

  getAttribute(name) {
    return this.attributes.has(name) ? this.attributes.get(name) : null;
  }

  hasAttribute(name) {
    return this.attributes.has(name);
  }

  removeAttribute(name) {
    this.attributes.delete(name);
  }

  /* ------------------------------------------------------------ queries */

  querySelectorAll(selector) {
    const found = [];
    const walk = (node) => {
      for (const child of node.children) {
        if (child.nodeType === 1 && matches(child, selector)) found.push(child);
        walk(child);
      }
    };
    walk(this);
    return found;
  }

  querySelector(selector) {
    return this.querySelectorAll(selector)[0] || null;
  }

  matches(selector) {
    return matches(this, selector);
  }

  closest(selector) {
    let node = this;
    while (node && node.nodeType === 1) {
      if (matches(node, selector)) return node;
      node = node.parentNode;
    }
    return null;
  }

  /* --------------------------------------------------------- shadow DOM */

  attachShadow(options = {}) {
    const root = new FakeNode(this.ownerDocument, 11, "#shadow-root");
    root.host = this;
    root.adoptedStyleSheets = [];
    root.shadowClosed = options.mode === "closed";
    this.shadowRootClosed = root;
    this.ownerDocument.shadowRoots.push(root);
    return root;
  }

  get shadowRoot() {
    if (!this.shadowRootClosed || this.shadowRootClosed.shadowClosed) return null;
    return this.shadowRootClosed;
  }

  /* --------------------------------------------------------- the layout */

  getBoundingClientRect() {
    const box = this.box || { left: 0, top: 0, width: 0, height: 0 };
    return {
      left: box.left,
      top: box.top,
      width: box.width,
      height: box.height,
      right: box.left + box.width,
      bottom: box.top + box.height,
    };
  }

  setBox(box) {
    this.box = { left: box.left || 0, top: box.top || 0, width: box.width || 0, height: box.height || 0 };
    return this;
  }

  get offsetHeight() {
    return this.box?.height || 0;
  }

  focus() {
    this.ownerDocument.activeElement = this;
  }

  /* ---------------------------------------------------------- listeners */

  addEventListener(type, listener, options) {
    if (typeof listener !== "function") return;
    const settings = typeof options === "boolean" ? { capture: options } : options || {};
    if (settings.signal?.aborted) return;

    if (!this.listeners.has(type)) this.listeners.set(type, []);
    const entry = { listener, capture: Boolean(settings.capture) };
    this.listeners.get(type).push(entry);

    settings.signal?.addEventListener(
      "abort",
      () => this.removeEventListener(type, listener, settings.capture),
      { once: true },
    );
  }

  removeEventListener(type, listener, options) {
    const capture = typeof options === "boolean" ? options : Boolean(options?.capture);
    const entries = this.listeners.get(type);
    if (!entries) return;
    const index = entries.findIndex((entry) => entry.listener === listener && entry.capture === capture);
    if (index >= 0) entries.splice(index, 1);
  }
}

/* -------------------------------------------------------------- the page */

export function createFakeDom(options = {}) {
  const dom = {
    observers: new Set(),
    resizeObservers: new Set(),
    shadowRoots: [],
    records: [],
    resourceEntries: [],
    timers: new Set(),
    bufferSize: 0,
  };

  const document = new FakeNode(null, 9, "#document");
  document.ownerDocument = document;
  document.title = options.title || "A Test Video - YouTube";
  document.fullscreenElement = null;
  document.hidden = false;
  document.activeElement = null;
  document.box = { left: 0, top: 0, width: 0, height: 0 };
  document.shadowRoots = dom.shadowRoots;

  document.createElement = (name) => new FakeNode(document, 1, String(name).toLowerCase());
  document.createElementNS = (_namespace, name) => new FakeNode(document, 1, String(name).toLowerCase());
  document.createTextNode = (text) => {
    const node = new FakeNode(document, 3, "#text");
    node.data = String(text);
    return node;
  };

  const html = document.createElement("html");
  const body = document.createElement("body");
  document.documentElement = html;
  document.body = body;
  document.querySelectorAll = (selector) => {
    const found = [];
    const walk = (node) => {
      for (const child of node.children) {
        if (child.nodeType === 1 && matches(child, selector)) found.push(child);
        walk(child);
      }
    };
    walk(document);
    return found;
  };
  document.querySelector = (selector) => document.querySelectorAll(selector)[0] || null;

  dom.document = document;
  dom.body = body;
  dom.html = html;

  dom.create = (tag) => {
    const element = document.createElement(tag);
    return element;
  };

  dom.mount = (element) => body.appendChild(element);

  /** A window the overlay can add listeners and read a size from. */
  const windowTarget = new FakeNode(document, 1, "#window");
  windowTarget.innerWidth = options.width || 1280;
  windowTarget.innerHeight = options.height || 800;
  windowTarget.window = windowTarget;
  dom.window = windowTarget;

  /** Report a structural change to the observers watching an ancestor. */
  dom.recordMutation = (target, addedNodes, removedNodes) => {
    dom.records.push({ type: "childList", target, addedNodes, removedNodes });
    if (dom.flushScheduled) return;
    dom.flushScheduled = true;
    queueMicrotask(() => {
      dom.flushScheduled = false;
      const records = dom.records.splice(0);
      for (const observer of dom.observers) {
        if (!observer.target) continue;
        observer.callback(records, observer);
      }
    });
  };
  document.recordMutation = dom.recordMutation;
  document.appendChild(html);
  html.appendChild(body);

  class FakeMutationObserver {
    constructor(callback) {
      this.callback = callback;
      this.target = null;
      dom.observers.add(this);
    }

    observe(target) {
      this.target = target;
    }

    disconnect() {
      this.target = null;
      dom.observers.delete(this);
    }
  }

  class FakeResizeObserver {
    constructor(callback) {
      this.callback = callback;
      this.elements = new Set();
      dom.resizeObservers.add(this);
    }

    observe(element) {
      this.elements.add(element);
    }

    unobserve(element) {
      this.elements.delete(element);
    }

    disconnect() {
      this.elements.clear();
      dom.resizeObservers.delete(this);
    }
  }

  class FakeStyleSheet {
    constructor() {
      this.cssText = "";
    }

    replaceSync(text) {
      this.cssText = String(text);
    }
  }

  /** Put a box on an element, as the page's layout would. */
  dom.layout = (element, box) => element.setBox(box);

  class FakePerformanceObserver {
    constructor(callback) {
      this.callback = callback;
      dom.performanceObservers.push(this);
    }

    observe() {}

    disconnect() {
      const index = dom.performanceObservers.indexOf(this);
      if (index >= 0) dom.performanceObservers.splice(index, 1);
    }
  }
  dom.performanceObservers = [];

  /** A resource the page asked for, announced to the observers. */
  dom.pushResource = (url) => {
    dom.addResource(url);
    const entries = [{ name: url, initiatorType: "media" }];
    for (const observer of dom.performanceObservers) {
      observer.callback({ getEntries: () => entries }, observer);
    }
  };

  /** A pointer and keyboard event that travels the real path. */
  dom.fire = (target, type, init = {}) => {
    const path = [];
    let node = target;
    let closedAt = -1;
    while (node) {
      path.push(node);
      if (node.shadowClosed) closedAt = path.length - 1;
      node = node.parentNode;
    }
    if (!path.includes(document)) path.push(document);
    if (!path.includes(windowTarget)) path.push(windowTarget);

    const event = {
      type,
      target,
      currentTarget: null,
      bubbles: init.bubbles !== false,
      composed: init.composed !== false,
      clientX: init.clientX ?? 0,
      clientY: init.clientY ?? 0,
      key: init.key,
      persisted: init.persisted,
      defaultPrevented: false,
      propagationStopped: false,
      insideShadow: false,
      // A closed shadow root is left out of the path a page may read; the
      // host is the first node such a listener sees.
      composedPath() {
        if (closedAt < 0 || this.insideShadow) return path.slice();
        return path.slice(closedAt + 1);
      },
      preventDefault() {
        this.defaultPrevented = true;
      },
      stopPropagation() {
        this.propagationStopped = true;
      },
      stopImmediatePropagation() {
        this.propagationStopped = true;
      },
    };
    dom.events.push(event);

    const call = (node, capture) => {
      const entries = node.listeners?.get(type);
      if (!entries || !entries.length) return;
      event.currentTarget = node;
      event.insideShadow = closedAt >= 0 && path.indexOf(node) <= closedAt;
      for (const entry of entries.slice()) {
        if (entry.capture !== capture) continue;
        entry.listener.call(node, event);
        if (event.propagationStopped) return;
      }
    };

    for (let index = path.length - 1; index > 0 && !event.propagationStopped; index -= 1) {
      call(path[index], true);
    }
    if (!event.propagationStopped) {
      call(target, true);
      call(target, false);
    }
    if (event.bubbles) {
      for (let index = 1; index < path.length && !event.propagationStopped; index += 1) {
        call(path[index], false);
      }
    }
    return event;
  };
  dom.events = [];

  /** A fullscreen change, as the browser announces it. */
  dom.setFullscreen = (element) => {
    document.fullscreenElement = element;
    dom.fire(document, "fullscreenchange", { bubbles: false });
  };

  /** A player that changed the size of its video. */
  dom.resize = () => {
    for (const observer of dom.resizeObservers) {
      observer.callback([...observer.elements], observer);
    }
  };

  /** A performance entry the page made. */
  dom.addResource = (url) => {
    dom.resourceEntries.push({ name: url, initiatorType: "media" });
  };

  let frameId = 0;
  const performance = {
    now: () => Number(process.hrtime.bigint() / 1_000_000n),
    getEntriesByType: (type) => (type === "resource" ? dom.resourceEntries.slice() : []),
    setResourceTimingBufferSize: (size) => {
      dom.bufferSize = size;
    },
  };

  const browser = {
    runtime: {
      sendMessage: async () => ({ ok: true }),
      onMessage: { addListener: (fn) => dom.messageListeners.push(fn) },
    },
    storage: {
      local: { get: async (defaults) => ({ ...defaults, ...dom.storedSettings }) },
      onChanged: {
        addListener: (fn) => dom.settingsListeners.push(fn),
        removeListener: (fn) => {
          const index = dom.settingsListeners.indexOf(fn);
          if (index >= 0) dom.settingsListeners.splice(index, 1);
        },
      },
    },
  };
  dom.messageListeners = [];
  dom.settingsListeners = [];
  dom.storedSettings = {};
  dom.browser = browser;

  const globals = {
    document,
    window: windowTarget,
    location: {
      href: options.url || "https://www.youtube.com/watch?v=abc",
      get hostname() {
        return new URL(this.href).hostname;
      },
    },
    performance,
    browser,
    CSSStyleSheet: FakeStyleSheet,
    MutationObserver: FakeMutationObserver,
    ResizeObserver: FakeResizeObserver,
    PerformanceObserver: FakePerformanceObserver,
    AbortController,
    console,
    URL,
    setTimeout,
    clearTimeout,
    // The registry sweeps on an interval; a check must be able to exit.
    setInterval: (fn, delay) => {
      const id = setInterval(fn, delay);
      id.unref?.();
      return id;
    },
    clearInterval,
    queueMicrotask,
    requestAnimationFrame: (fn) => {
      frameId += 1;
      const id = setTimeout(() => fn(performance.now()), 0);
      dom.timers.add(id);
      return id;
    },
    cancelAnimationFrame: (id) => {
      dom.timers.delete(id);
      clearTimeout(id);
    },
  };
  dom.globals = globals;

  /** Let timers, frames and microtasks run. */
  dom.settle = async (times = 4) => {
    for (let index = 0; index < times; index += 1) {
      await new Promise((resolve) => setTimeout(resolve, 2));
      await Promise.resolve();
    }
  };

  return dom;
}
