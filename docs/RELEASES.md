# Release, installer and update notes

How this application ships on Windows, Linux and macOS, and what was verified
before the first release. The in-app update path and the release pipeline are
wired together; changing one without the other is the main way this can break.

## Versioning — one source of truth

`package.json` holds the release version. `scripts/bump-version.mjs` copies it
into `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, the workspace
`Cargo.lock` and the version badge in `README.md`. Never edit those version
fields by hand; run the release script instead. The running app reads its
version from the bundle, so the About box, the installer, the GitHub release
and `latest.json` always agree. `scripts/check-release-integrity.mjs` asserts
this on every pull request and again on the tag.

## Making a release

```
pnpm release:patch        # or :minor; :major only from 1.x
```

Before running it, add a section to `WHATS_NEW.md` for the new version:

```md
## 0.3.0 — 2 November 2026

### Downloads

- Downloads recover more reliably after a dropped connection.
- A finished file can be opened from the completion window.
```

`WHATS_NEW.md` is the single source of truth for reader-facing notes. The
release script refuses to tag a version that has no section for it, and the
release workflow uses that section as the GitHub release body and as the
`latest.json` notes. The app also bundles the file and shows the matching
section in its What's New popup, so the release page and the app cannot drift
apart. Write plain sentences for someone who never reads the changelog; do not
copy commit subjects.

The command refuses a dirty worktree, a duplicate tag, a missing `origin`, or
a missing `WHATS_NEW.md` section. It bumps, generates the technical
`CHANGELOG.md` section from the conventional commits since the last tag
(`feat:` Added, `fix:` Fixed, `perf:` Performance, `!` or `BREAKING CHANGE:`
first under Breaking changes), commits, tags `vX.Y.Z` and pushes both. The
pushed tag starts `.github/workflows/release.yml`, which runs the pipeline in
three stages:

1. **verify** — the same contracts CI enforces (version agreement in every
   file, the updater key and endpoint, the `WHATS_NEW.md` section for this
   exact tag, the bridge constants shared with both extensions), plus the
   typecheck, the frontend unit tests and every self-check script. Nothing is
   built if this job is red.
2. **build** — the signed Windows NSIS installer, the Linux `deb` + AppImage
   and the universal macOS `app` + `dmg`, uploaded with their `.sig` files and
   a merged `latest.json`. The jobs run one after the other so the shared
   `latest.json` is not overwritten, and the GitHub release is created as a
   **draft** — installed apps cannot see a draft, because the updater only
   resolves published releases.
3. **verify-artifacts** — downloads the draft's own assets back and verifies
   every signature against the public key baked into the build, and checks
   that `latest.json` names those files for all three platforms.

When `verify-artifacts` is green, open the draft on GitHub, read the notes and
press **Publish release**. That click is the only thing that ships an update to
installed copies. To re-run a release that failed, use
`gh workflow run release.yml --ref vX.Y.Z`; `verify` runs again from the top.

Check the words of a release before pushing one you care about: add the
`WHATS_NEW.md` section, run `pnpm check:whats-new`, and read the rendered
notes. `CHANGELOG.md` remains the technical record for maintainers; it is not
what the reader sees in the app popup.

One-time setup on GitHub (secrets for the repository, Settings → Secrets →
Actions):

- `TAURI_SIGNING_PRIVATE_KEY` — the contents of `~/.tauri/bdownload.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — an empty string (this key has no
  password)

The matching public key is baked into `src-tauri/tauri.conf.json` under
`plugins.updater.pubkey`. If the private key is ever lost, generate a new pair
(`pnpm tauri signer generate -w ~/.tauri/bdownload.key -p "" --ci`), put the
new public key in the config **and release a new version** — installed copies
verify updates against the key that was baked into *them*.

## Update flow (what is trusted)

1. Help → Check for updates asks exactly one endpoint, compiled into the app:
   `https://github.com/bilals2008/b-download-manager/releases/latest/download/latest.json`.
   GitHub only serves that path for published, non-draft releases.
2. The updater plugin verifies the `latest.json` signature and then the
   minisign signature of the downloaded package against the baked public key.
   A package the key did not sign is refused before it touches disk.
3. Nothing installs until the reader presses *Update now* on the notice or in
   the update dialog; the manual dialog shows the release notes first.
4. The window shows the download's own byte count and the verification and
   install stages. Immediately before the installer starts, running downloads
   are paused and saved the same way the close button saves them; then the
   installer takes over, reuses the recorded folder and restarts the app.

If the repository is renamed, the endpoint in `tauri.conf.json` must change
with it — old installs would otherwise stop seeing updates.

## Installer behaviour on Windows

- Per-user install (`currentUser`): no UAC prompt, updates can replace it
  without elevation, and the app data lives under the same trust level.
- The first launch shows the standard NSIS **Choose Install Location** page
  (`MUI_PAGE_DIRECTORY`) with `%LOCALAPPDATA%\B Download Manager` as the
  default. The reader can browse to any folder; the installer writes the
  chosen path to `HKCU\Software\bilals2008\B Download Manager` and to the
  uninstall entry's `InstallLocation`. Every later installer run starts from
  that value (`RestorePreviousInstallLocation`), so a reinstall or upgrade
  keeps the original folder.
- An in-app update runs the same installer with `/P /UPDATE /R`. Passive mode
  skips every page, `/UPDATE` skips the reinstall question and the uninstall
  step, and the restored registry path is the folder the app already lives in.
  The reader is never asked to choose a folder during a normal update, and no
  second copy is installed.
- The wizard is branded from `src-tauri/nsis/header.bmp` (150×57),
  `src-tauri/nsis/sidebar.bmp` (164×314) and `src-tauri/icons/icon.ico`.
  `scripts/check-release-integrity.mjs` fails a release when one of those
  files is missing, so a build cannot silently fall back to the Tauri artwork.
- The NSIS template creates the Start Menu shortcut (a Desktop shortcut is
  offered by the installer UI itself in current Tauri versions — verified
  against the generated `.nsi` for this build).
- Settings live in WebView2 storage under `%APPDATA%\com.bilal.bdownloadmanager`
  and the download list/engine state in the same folder. The uninstaller does
  not delete it: upgrading keeps everything, and even an uninstall leaves the
  history for a reinstall (verified against the generated `.nsi`).
- "Run on startup" is opt-in in Settings → General and uses the registry
  startup entry via the autostart plugin; the installer never enables it by
  itself.

## Update notification and progress

- The startup check follows Settings → About → *Check for updates when the
  application starts* (off by default). Help → Check for updates and the
  About page are always available for a manual check.
- A newer release appears in the small notice in the bottom-right corner, not
  in a window that takes the keyboard. *Update now* starts the download;
  *Later* writes the version to `dismissedUpdateVersion` and the same release
  is not offered again. A newer release is a new question.
- The notice, the manual dialog and the About page read one shared install
  state. The backend streams real response bytes over a Tauri channel and the
  server's `Content-Length` when it sent one; an unknown total runs an
  indeterminate segment instead of a made-up percentage. The stages are
  downloading → verifying → installing, and the restart offer appears only
  after the installer has been started. A failure keeps the backend's sentence
  and offers *Try again*.
- Running downloads keep running while the package downloads. They are paused
  and saved immediately before the installer starts, the same way the close
  button saves them, so unfinished files and the download list survive the
  restart.
- The byte line under the meter is the small caption size and carries the
  measured transfer rate beside the received and total sizes: the rate is the
  bytes gained across the last second of backend reports, not a timer.

## Installer behaviour on Linux

- The workflow builds a `deb` package for Debian/Ubuntu and an AppImage for
  any distribution. Both carry the same bundle identifier and app data folder.
- The AppImage is the artifact the in-app updater uses on Linux; `latest.json`
  carries its signature under the `linux-x86_64` keys.
- Building on Ubuntu 22.04 keeps the AppImage usable on older glibc systems.

## Installer behaviour on macOS

- One universal `.dmg` covers Apple silicon and Intel; the same binary is
  published under both `darwin-aarch64` and `darwin-x86_64` in `latest.json`.
- The updater payload is the `.app.tar.gz` produced by the `app` bundle, so the
  release workflow builds `--bundles app,dmg`. A dmg-only build leaves
  `latest.json` without a darwin entry and Macs never update (v0.2.0 shipped
  that way; the gate now fails a release that has no macOS payload).
- The build is not signed with an Apple Developer ID and not notarised, so the
  first launch needs right-click → **Open**. Signing is the next step before a
  1.0; it needs a paid Apple Developer account.

## Verified / not verified

Verified locally (November 2026, CI/CD audit):

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace --no-fail-fast`: 171 engine tests (2 ignored) and
  274 application tests pass from one workspace.
- The release profile survived the workspace move: a `--release` build carries
  `lto`, `opt-level=s`, `codegen-units=1`, `panic=abort` and `strip`.
- `pnpm test` (120 frontend tests), `pnpm check:all` (all 20 self-checks) and
  `pnpm exec tsc --noEmit` pass, and the release-integrity gate rejects a
  mismatched tag or version.
- The v0.3.1 NSIS installer was built locally with the branded header, sidebar
  and icon. `pnpm check:installer` read the generated `.nsi` and passed:
  per-user scope, `MUI_PAGE_DIRECTORY`, its passive-mode skip, the recorded
  install folder and `RestorePreviousInstallLocation` were all present.
- The Welcome and Choose Install Location pages were opened and read on this
  machine: the sidebar carries the mark and product name, the header carries
  the app tile, the default folder is `%LOCALAPPDATA%\B Download Manager`, and
  Browse is offered.
- A temporary install record (backed up and restored afterwards) was used for
  the Windows install scenarios: a fresh silent install to a custom folder put
  the executable and uninstaller there, recorded that folder and
  pointed the Start Menu shortcut at it. Re-running the installer with the
  updater's `/P /UPDATE` replaced the executable, kept the custom folder,
  kept a marker file and the settings file byte-for-byte, and left the real
  installation untouched. The uninstaller removed the test folder and the
  registry and shortcuts were restored.
- The published v0.3.1 installer verifies against the baked key
  `488BA63203E8DB50`; so does the published v0.2.0 asset. The local
  updater-artifact signature step could not run in this workspace because the
  local private key is password-protected and that password is not available
  here. CI signs with `TAURI_SIGNING_PRIVATE_KEY`; the command and public key
  were not changed.
- `scripts/check-release-artifacts.mjs` verified the **published** v0.2.0
  Windows installer against the key baked into `tauri.conf.json` (key id
  `488BA63203E8DB50`): the ed25519 check over the BLAKE2b-512 digest succeeds,
  and flipping one byte of the installer makes it fail. The same script reads
  the signatures inside `latest.json` and proves they match the assets.
- The v0.2.0 release itself exposed two gaps this pipeline now closes: the
  release body was the CHANGELOG fallback (no WHATS_NEW section at tag time),
  and the macOS dmg shipped without an updater payload. `verify` refuses the
  first and `build --bundles app,dmg` fixes the second.

Not yet observed on GitHub:

- This exact CI/release workflow has not run on a tag yet; the first release
  after this change is the end-to-end proof. Publish it as a draft and check
  `verify-artifacts` before pressing Publish.

Still needs a published higher version (or a second machine):

- A real *Check for updates* round trip from an installed copy through the
  signed download, the progress card, the passive installer and the restart.
  The installer mechanics and the artifact signatures were verified; the
  network round trip needs a release that is newer than the installed copy.
- The update notice, progress card and restart-ready state were tested in
  component/store tests and typechecked, but not observed in the running app
  against a live newer release.
- Uninstall keeps `%APPDATA%\com.bilal.bdownloadmanager`; this was verified
  against the generated script and the temporary install did not touch that
  folder, but a real uninstall/reinstall of the user's data is left for the
  release machine.
- An update published from a *different* key is refused (the security gate
  itself): the artifact verifier rejects a mismatched key, though the running
  updater has not yet been put through that case.
