# Debrid accounts (Real-Debrid, AllDebrid, Premiumize)

> **Nov 2026:** implemented (Settings -> Services, add dialog switch, unlock before start). See REMAINING-FEATURES.md for the current proof.

**Wave 5 · Status:** 🔧 `crates/idm-debrid` exists, not a dependency of the app

## Goal

A file-host link the user's debrid account can unlock downloads at full speed
through that account, without the link ever appearing in the log.

## Where it stands

`crates/idm-debrid` has the services (`real_debrid.rs`, `all_debrid.rs`,
`premiumize.rs`), a credential store, a rate limiter and tests. `src-tauri`
does not depend on it, so no command can reach it. There is no settings page.

## Steps

1. Add the crate to `src-tauri` and one command module `src-tauri/src/debrid.rs`
   (sign in, sign out, account state, services).
2. Never put the key in `settings.json` or in `localStorage`: use
   `idm_debrid::credential`'s store, which is the OS credential vault. The
   settings page shows only "signed in as …".
3. Detect what the engine should do with a link: an "unlock this link" toggle in
   the add dialog, and an automatic pass when the host matches a service the
   account supports.
4. Unrestrict before the start, in the backend, so a browser hand-over gets it
   too; the download then starts on the generated URL with its filename and
   size.
5. Respect the published limits with the crate's rate limiter (Real-Debrid
   250/min, AllDebrid 12/s and 600/min) and surface a 429 as a sentence, not a
   failed download.
6. Keep the generated URL out of the log and out of `downloads.json`; it is a
   credential for as long as it lives. Re-unrestrict on resume if it expired.
7. Say "experimental" in the settings page, as the reference build does.

## Files

`src-tauri/Cargo.toml`, `src-tauri/src/debrid.rs`, `src-tauri/src/lib.rs`,
`src-tauri/src/actions.rs` (unrestrict before start),
`src/components/settings/*` (a new service page + search entries),
`src/components/workspace/add-download-dialog.tsx`, `src/lib/ipc.ts`.

## Edge cases

- An account with no premium days left: say that, do not start a slow download.
- A service that is down: the download may still start on the original link if
  the user says so.
- Two downloads unlocking at once must go through the limiter, not race.

## Tests

Unit: the link-to-service rule and the "should this be unlocked" decision.
Integration: a fake service server, one unlock, one download of the generated
URL. Secret test: a log written during a failure contains no key.

## Proof

`cargo test -p idm-debrid -p my-idm-clone`, a real account in the running app,
and `grep` over the log and the index for the key: nothing.

## Out of scope

Torrents through debrid, and usenet.
