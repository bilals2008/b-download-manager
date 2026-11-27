# Firefox build

This folder adds the Firefox manifest to the shared source in `../source/`.
Firefox runs a Manifest V3 **event page** (`background.scripts`), which is what
Firefox supports in MV3; the same scripts also run in Chromium behind
`../source/lib/platform.js`, so there is one implementation, not two.

```powershell
node extensions/build.mjs firefox
```

## Load it

1. Start **B Download Manager**. It opens the loopback bridge on
   `127.0.0.1:27500` with itself.
2. Open `about:debugging#/runtime/this-firefox`.
3. **Load Temporary Add-on…** → `extensions/dist/firefox/manifest.json`.

Temporary add-ons are removed when Firefox closes. A release Firefox only
installs signed add-ons:

```powershell
pnpm pack:firefox
npx web-ext sign --source-dir extensions/dist/firefox --channel unlisted
```

Install the resulting `.xpi` from `about:addons` → gear → **Install Add-on From
File**. A self-distributed build needs Developer Edition or Nightly with
`xpinstall.signatures.required` turned off.

## Publishing notes

The add-on id is `idm-bridge@my-idm-clone.local`, `strict_min_version` is 142,
and `data_collection_permissions` declares `none`. Validate before submitting:

```powershell
npx web-ext lint --source-dir extensions/dist/firefox --self-hosted
```

## What Firefox gets

The full integration: right-click hand-over, download takeover, the video
button with its quality panel and live progress, the Download all links window,
the popup with live downloads, and the settings page. See
[`../README.md`](../README.md) for the whole list and the bridge contract.
