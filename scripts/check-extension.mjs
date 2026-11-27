// Self-check and unit tests for the Firefox extension.
//
// The extension is plain scripts, so the checks run the files the browser
// loads, not copies: the manifest is read, every page reference is resolved,
// the settings and media helpers are executed in a sandbox, and the background
// page is loaded against a mocked browser and a mocked loopback bridge. Run it
// with `pnpm check:extension`.
//
// The checks live in ./check-extension/, one module per responsibility. They
// are imported in order below; each runs its checks as it loads, and the shared
// sandbox state is set up before the modules that drive it.

await import("./check-extension/manifest.mjs");
await import("./check-extension/files.mjs");
await import("./check-extension/pages.mjs");
await import("./check-extension/links-wiring.mjs");
await import("./check-extension/theme.mjs");
await import("./check-extension/icons.mjs");

await import("./check-extension/sandbox.mjs");
await import("./check-extension/settings.mjs");
await import("./check-extension/link-rules.mjs");

await import("./check-extension/page-links.mjs");
await import("./check-extension/media.mjs");
await import("./check-extension/page-sources.mjs");

await import("./check-extension/bridge.mjs");
await import("./check-extension/background.mjs");
await import("./check-extension/browsers.mjs");

// The overlay needs a page rather than a sandbox: scripts/check-overlay.mjs
// runs the content scripts in a fake one and exercises the button there.
await import("./check-overlay.mjs");

// The link window is a page of its own: scripts/check-links.mjs runs it
// against a small document and exercises the list, the filters and a send.
await import("./check-links.mjs");

console.log("extension: all checks pass");