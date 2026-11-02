// The environment every component test starts from. Three pieces of browser
// API that jsdom does not implement are stubbed here, and the Tauri command
// bridge is replaced before any module imports it (several modules call
// `invoke` at module scope).
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

window.matchMedia ??= ((query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addListener: () => {},
  removeListener: () => {},
  addEventListener: () => {},
  removeEventListener: () => {},
  dispatchEvent: () => false,
})) as typeof window.matchMedia;

class NoopResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

class NoopIntersectionObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
  takeRecords() {
    return [];
  }
}

Object.assign(globalThis, {
  ResizeObserver: NoopResizeObserver,
  IntersectionObserver: NoopIntersectionObserver,
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});
