# Safari build

Safari 16.4 and newer runs web extensions, and it exposes the promise-shaped
`browser` API the shared source is written against, so this folder adds only a
manifest: no adapter, no service-worker entry.

```powershell
node extensions/build.mjs safari
```

## Wrap it in an app (macOS)

A Safari extension ships inside a containing app, and the wrapper is made by
Apple's tooling:

```bash
xcrun safari-web-extension-converter extensions/dist/safari
```

That writes an Xcode project that embeds the folder. Build and run it once, then
open Safari → **Settings → Extensions** and turn the extension on. For
development, Safari also needs **Develop → Allow Unsigned Extensions** after each
launch.

Without a Mac the folder is still the complete extension source: `manifest.json`
is valid, every file the manifest names is in the folder, and `pnpm
check:extension` verifies both.

## What differs from Firefox

- The background is a non-persistent document (`background.scripts`), which is
  the form Safari runs reliably in MV3, rather than a service worker. The script
  list is the Firefox list, unchanged.
- `permissions` leaves out `tabs`: the popup reads the active tab it was opened
  on, and the content scripts already know their own page.
- `strict_min_version` is 16.4, the first Safari with MV3 and the `browser`
  namespace this source uses.
- Distribution goes through the App Store or a signed app; there is no
  self-hosted `.safariwebextension` a reader can double-click.
