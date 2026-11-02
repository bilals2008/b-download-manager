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
3. Nothing installs until the reader presses "Download and install" in the
   dialog; the release notes are shown first.
4. Running downloads are paused and saved the same way the close button saves
   them, then the installer takes over and the app restarts.

If the repository is renamed, the endpoint in `tauri.conf.json` must change
with it — old installs would otherwise stop seeing updates.

## Installer behaviour on Windows

- Per-user install (`currentUser`): no UAC prompt, updates can replace it
  silently, and the app data lives under the same trust level.
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
  265 application tests pass from one workspace.
- The release profile survived the workspace move: a `--release` build carries
  `lto`, `opt-level=s`, `codegen-units=1`, `panic=abort` and `strip`.
- `pnpm test` (30 frontend tests), `pnpm check:all` (all 20 self-checks) and
  `pnpm exec tsc --noEmit` pass, and the release-integrity gate rejects a
  mismatched tag or version.
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

Needs a second machine (a clean Windows install without the dev app):

- Fresh install → shortcuts → first-run notice → settings survive an in-app
  update to a higher version → paused downloads resume after it.
- Uninstall keeps `%APPDATA%\com.bilal.bdownloadmanager`; a reinstall shows
  the old list.
- An update published from a *different* key is refused (the security gate
  itself).
- A real "Check for updates" round trip from an installed copy to a higher
  published version still needs a second machine.
