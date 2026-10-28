# Release, installer and update notes

How this application ships on Windows, Linux and macOS, and what was verified
before the first release. The in-app update path and the release pipeline are
wired together; changing one without the other is the main way this can break.

## Versioning — one source of truth

`package.json` holds the release version. `scripts/bump-version.mjs` copies it
into `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and
`src-tauri/Cargo.lock`. Never edit those version fields by hand; run the
release script instead. The running app reads its version from the bundle, so
the About box, the installer, the GitHub release and `latest.json` always
agree.

## Making a release

```
pnpm release:patch        # or :minor; :major only from 1.x
```

The command refuses a dirty worktree, a duplicate tag, or a missing `origin`.
It bumps, generates the `CHANGELOG.md` section from the conventional commits
since the last tag (`feat:` Added, `fix:` Fixed, `perf:` Performance, `!` or
`BREAKING CHANGE:` first under Breaking changes), commits, tags `vX.Y.Z` and
pushes both. The pushed tag starts `.github/workflows/release.yml`, which
builds the signed Windows NSIS installer, the Linux `deb` + AppImage and the
universal macOS dmg, uploads them with their `.sig` files, merges all three
platforms into `latest.json` and publishes the GitHub release with the
changelog section as its body and its notes. The platform jobs run one after
the other so the shared `latest.json` is not overwritten.

Check the words of a generated section before pushing a release you care
about: `git show` the bump commit and amend it, or edit `CHANGELOG.md` and
tag by hand (`git tag -a vX.Y.Z -m … && git push origin vX.Y.Z`).

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
- The build is not signed with an Apple Developer ID and not notarised, so the
  first launch needs right-click → **Open**. Signing is the next step before a
  1.0; it needs a paid Apple Developer account.

## Verified / not verified

Verified locally (October 2026, before v0.1.0):

- `cargo fmt --check`, `cargo clippy -D warnings`, full `cargo test`.
- Frontend typecheck, `pnpm build`, all `check:*` scripts.
- The signed release build produces `B Download Manager_0.1.0_x64-setup.exe`
  plus its `.sig`, and a `latest.json`; the bundle identifier is
  `com.bilal.bdownloadmanager`.
- The built `.sig` was verified directly against the public key baked into
  `tauri.conf.json`: the key ids match (`E7416FBE5836CDE6`) and an ed25519
  verify over the BLAKE2b-512 digest of the installer succeeds. A single
  flipped byte in the installer makes the same check fail, so the artifact on
  disk is genuinely bound to its signature.
- The plugin's own verifier suite (`tauri-plugin-updater`) passes, including
  the vectors that assert a tampered package is refused.

Needs a second machine (a clean Windows install without the dev app):

- Fresh install → shortcuts → first-run notice → settings survive an in-app
  update to a higher version → paused downloads resume after it.
- Uninstall keeps `%APPDATA%\com.bilal.bdownloadmanager`; a reinstall shows
  the old list.
- An update published from a *different* key is refused (the security gate
  itself).
- The tag-triggered workflow ran end to end for v0.1.0: it published the
  Windows NSIS installer and the Linux `deb` + AppImage, each with its `.sig`,
  and a `latest.json` carrying the Windows and Linux platform entries.
- A real "Check for updates" round trip from an installed copy to a higher
  published version still needs a second machine.
