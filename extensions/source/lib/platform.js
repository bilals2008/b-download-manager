/*
  One source, several engines.

  Firefox and Safari expose `browser`, whose methods answer with promises.
  Chromium exposes `chrome`, whose methods take a callback. This file fills in
  whichever one the running engine is missing, so every other script in this
  folder is written once, against `browser`, and runs unchanged everywhere.

  It is loaded first in the Chromium build only. Firefox and Safari have the
  promise API already, so loading it there would add a layer that then has to
  be kept honest against the native one; their manifests and pages leave it
  out, and the guard below makes even an accidental load a no-op.
*/

(() => {
  if (globalThis.browser || !globalThis.chrome) return;

  const chrome = globalThis.chrome;

  /**
   * A callback-style API answered as a promise.
   *
   * `chrome.runtime.lastError` has to be read inside the callback or the
   * browser logs an unchecked-error warning nobody can act on.
   */
  function promiseFrom(owner, name) {
    return (...args) =>
      new Promise((resolve, reject) => {
        owner[name](...args, (value) => {
          const failure = chrome.runtime.lastError;
          if (failure) reject(new Error(failure.message));
          else resolve(value);
        });
      });
  }

  /** The promise version, or a quiet stand-in when the engine lacks the API. */
  function when(owner, name, fallback) {
    return owner && typeof owner[name] === "function"
      ? promiseFrom(owner, name)
      : fallback;
  }

  const noop = async () => undefined;
  const runtime = chrome.runtime;
  const action = chrome.action || chrome.browserAction || {};
  const storage = chrome.storage?.local;

  /**
   * A Chromium message listener answers with `sendResponse` and `true`; the
   * other engines also accept a listener that returns a promise. Adapting it
   * here is what lets one handler serve every build.
   */
  function messageChannel(register) {
    return {
      addListener(listener) {
        register.addListener((message, sender, sendResponse) => {
          let answer;
          try {
            answer = listener(message, sender, sendResponse);
          } catch (error) {
            console.error("B Download Manager listener failed:", error);
            return false;
          }
          if (!answer || typeof answer.then !== "function") return false;
          answer.then(sendResponse, (error) => {
            sendResponse({
              ok: false,
              error: {
                kind: "unknown",
                message: String(error?.message || error),
              },
            });
          });
          return true;
        });
      },
      removeListener: (listener) => register.removeListener?.(listener),
      hasListener: (listener) => Boolean(register.hasListener?.(listener)),
    };
  }

  globalThis.browser = {
    runtime: {
      id: runtime.id,
      getURL: runtime.getURL.bind(runtime),
      getManifest: runtime.getManifest.bind(runtime),
      sendMessage: promiseFrom(runtime, "sendMessage"),
      openOptionsPage: when(runtime, "openOptionsPage", noop),
      onMessage: messageChannel(runtime.onMessage),
      onInstalled: runtime.onInstalled,
      onStartup: runtime.onStartup,
    },
    storage: {
      local: {
        get: when(storage, "get", async (defaults) => ({ ...defaults })),
        set: when(storage, "set", noop),
        remove: when(storage, "remove", noop),
      },
      onChanged: chrome.storage?.onChanged,
    },
    action: {
      setBadgeText: when(action, "setBadgeText", noop),
      setBadgeBackgroundColor: when(action, "setBadgeBackgroundColor", noop),
      // Chrome 110 and newer; the overlay asks with `?.`, so an older engine
      // simply keeps its own badge text colour.
      setBadgeTextColor: when(action, "setBadgeTextColor", undefined),
    },
    contextMenus: chrome.contextMenus
      ? {
          create: when(chrome.contextMenus, "create", () => {}),
          removeAll: when(chrome.contextMenus, "removeAll", noop),
          onClicked: chrome.contextMenus.onClicked,
        }
      : undefined,
    tabs: chrome.tabs
      ? {
          query: promiseFrom(chrome.tabs, "query"),
          create: promiseFrom(chrome.tabs, "create"),
          sendMessage: when(chrome.tabs, "sendMessage", async () => undefined),
          onUpdated: chrome.tabs.onUpdated,
          onRemoved: chrome.tabs.onRemoved,
        }
      : undefined,
    windows: chrome.windows
      ? { create: promiseFrom(chrome.windows, "create") }
      : undefined,
    downloads: chrome.downloads
      ? {
          pause: promiseFrom(chrome.downloads, "pause"),
          resume: promiseFrom(chrome.downloads, "resume"),
          cancel: promiseFrom(chrome.downloads, "cancel"),
          erase: promiseFrom(chrome.downloads, "erase"),
          onCreated: chrome.downloads.onCreated,
        }
      : undefined,
    cookies: chrome.cookies
      ? { getAll: promiseFrom(chrome.cookies, "getAll") }
      : undefined,
    // `alarms.create` is fire-and-forget in every engine; wrapping it in a
    // promise that never settles would be worse than leaving it alone.
    alarms: chrome.alarms
      ? {
          create: chrome.alarms.create.bind(chrome.alarms),
          onAlarm: chrome.alarms.onAlarm,
        }
      : undefined,
  };
})();
