# Remaining features

Everything compared against `D:\Dev\misterdownloadmanager` (MDM, the reference
build) and what is still to do here. The waves are the ones in
`.agents/WORKFLOW.md`; a line leaves this file when it works in the running app
and the probe proves it.

Legend: ✅ done · 🔧 the engine code exists but nothing in the app can reach it
· ❌ not written yet

## Done

- ✅ The progress and completion windows: labels `progress` / `complete` must be
  in `src-tauri/capabilities/default.json`. Missing there, they open but cannot
  be dragged or closed and never hear `download://update` — the freeze.
- ✅ Checksum verification of a finished file (`src-tauri/src/checksum.rs`,
  Properties → Checksum). `is_hash` no longer miscounts SHA-1/SHA-512.
- ✅ Archive extraction of a finished file (`src-tauri/src/extract.rs`,
  row menu → Extract here).

## Wave 1 — splits (blocks parallel work)

- 🔧 22 files over 800 lines, listed in `scripts/check-file-size.mjs`. Largest:
  `crates/idm-core/src/downloader/tests.rs` 2845, `downloader.rs` 2272,
  `video/mp4.rs` 1805, `video/verify.rs` 1707, `src/state/download-store.tsx`
  1640, `src-tauri/src/lib.rs` 1515, `state.rs` 1513, `net/pac/js.rs` 1498.
  Move code, never rewrite it; `cargo test --workspace` + `pnpm test` prove it.

## Wave 2 — engine reliability

- ❌ Per-host learned connection caps: remember what a host actually allows and
  cap the next download to it (IDM opens 480 on a 2-connection server; MDM keeps
  the cap it learned).
- ❌ Slow-connection shedding: drop a range that is far behind and give its
  bytes to a faster connection.
- ❌ Connection rebalancing until the last byte (partly in `segmenter.rs`).
- 🔧 Server location per download: country flag + RTT. `crates/idm-core/src/geo.rs`
  and `geo/ipcountry.bin.xz` exist; `DownloadInfo` has no field and the progress
  window shows nothing. MDM: `crates/geo`, `crates/app/src/countries.rs`.
- 🔧 Freeze detector beside the crash monitor: `crates/idm-crash` writes a report
  when the process ends badly; a window that stops answering is not detected.

## Wave 3 — the wire

- 🔧 Encrypted DNS: DoH, DoT, NextDNS profile. `crates/idm-core/src/net/dns.rs`
  exists and nothing calls it. Settings → Connection has no page for it.
- 🔧 Network adapter binding with kill switch: `net/adapter.rs` exists unwired.
  Windows needs `IP_UNICAST_IF` / `IPV6_UNICAST_IF` plus `NotifyIpInterfaceChange`
  to notice the adapter leaving; on loss, fail closed.
- 🔧 PAC proxy execution: `net/pac.rs` (+ `net/pac/js.rs`, 1499 lines) exists
  unwired. MDM evaluates PAC per URL (`WinHttpGetProxyForUrl`); a cached global
  route is wrong.

## Wave 4 — media

- 🔧 Soft subtitles into MP4 (`mov_text`) and MKV (`S_TEXT/UTF8`):
  `crates/idm-core/src/media/subtitles.rs` exists, nothing references it. Needs
  a pinned, checksum-verified FFmpeg (the yt-dlp helper is the model), auto or
  manual captions, `-c:v copy`.
- 🔧 Audio extraction / conversion to mp3, opus, flac:
  `crates/idm-core/src/media/audio.rs` exists unwired. B downloads the track
  as-is; conversion never re-encodes video.
- ❌ gallery-dl image galleries: absent. MDM: `crates/tools/src/gallery.rs`.
- ❌ Google Drive / Docs special handling: absent. MDM:
  `crates/engine/src/sites/gdrive.rs`.
- 🔧 DRM detection: only a content-type refusal in `video/capture.rs`. MDM
  refuses `#EXT-X-KEY`, MPD `ContentProtection`, `vnd.yt-ump`, encrypted init —
  detection only, never a bypass.

## Wave 5 — the outside world

- 🔧 Debrid accounts (Real-Debrid, AllDebrid, Premiumize; TorBox in MDM is web
  downloads only). `crates/idm-debrid` exists but is **not a dependency of
  `src-tauri`**, so nothing can call it. Needs: dependency, a settings page,
  the key in the OS credential store (never `settings.json`, never a log),
  unrestrict before the start, rate limits respected (RD 250/min, AD 12/s).
- ✅ Archive handling, first half: `idm-archive` is a dependency of the app and
  a finished archive can be unpacked from the row menu (**Extract here**), into
  a folder of its own name, through the crate's traversal-safe policy. Still
  to do: the extract-when-done setting, removing the archive after a verified
  extract, and the password prompt for an encrypted archive. 7z and RAR go
  through 7-Zip when it is installed.

## Also in MDM, not in your list

Kept here so nothing is lost. Each is a crate MDM has and this project lacks:

- ❌ BitTorrent (magnet links, `.torrent`, seeding) — MDM: `crates/torrent` on
  librqbit.
- ❌ MCP server for AI assistants — MDM: `crates/mcp`.
- ❌ Phone / LAN remote control — MDM: `crates/remote`.
- ❌ Queue scheduler (start and stop on a clock, wake the machine) — MDM:
  `crates/app/src/queue.rs`.
- ❌ 86 languages and right-to-left layouts — MDM: `i18n/`.
- ❌ Portable build and a headless server mode — MDM: `docker/`, `--headless`.

## How each one is proven

1. `pnpm verify` — every self-check, the type check, the unit tests.
2. `cargo test --workspace`.
3. `pnpm tauri build --no-bundle`, then run `target/release/my-idm-clone.exe`.
4. `pnpm probe:bridge` and `pnpm probe:freeze` against it, plus the window
   itself driven by hand for anything that draws.

## Wave 2, done

- ✅ **Per-host learned caps** — `LearnedCaps` in `crates/idm-core/src/limits.rs`
  records what a host honoured, keeps it in `<data>/host-caps.json`, and starts
  the next download from it. Shown in the progress window's connection details.
  Reset is engine-only for now (no button).
- ✅ **Slow-connection shedding** — `crates/idm-core/src/pace.rs` decides which
  range stopped moving; its tail is handed to a new connection, capped at four
  hand-overs per run.
- ✅ **Rebalancing** — a range that lags the median by a quarter gets the free
  slot when one comes free, so the last byte is not fetched alone.
- ✅ **Server country and RTT** — `geo::measure` runs beside the transfer and
  reports the country code and the round trip into the status tab.
- ✅ **Freeze detector** — the monitor asks the main window whether it answers
  (`SendMessageTimeoutW`), so a suspended machine is not called frozen and a
  stuck message loop is reported once per episode.

## The plans

One file per feature, in `planning/features/`. Each says where it stands, the
steps in order, the files it touches, the edges, the tests and the proof it
needs. No code is written in them on purpose: they are the shape of the work.

| Plan | Feature | Wave |
|---|---|---|
| `features/01-file-splits.md` | every file under 800 lines | 1 |
| `features/02-per-host-connection-caps.md` | learned caps per host | 2 |
| `features/03-slow-connection-shedding.md` | shedding a stalled range | 2 |
| `features/04-connection-rebalancing.md` | rebalancing to the last byte | 2 |
| `features/05-server-location.md` | country flag and RTT | 2 |
| `features/06-encrypted-dns.md` | DoH, DoT, NextDNS | 3 |
| `features/07-adapter-binding-kill-switch.md` | adapter binding and kill switch | 3 |
| `features/08-pac-proxy.md` | PAC per URL | 3 |
| `features/09-freeze-detector.md` | freeze detector and report | 2 |
| `features/10-debrid-accounts.md` | Real-Debrid, AllDebrid, Premiumize | 5 |
| `features/11-archive-handling.md` | the rest of the archive work | 5 |
| `features/12-subtitles.md` | captions into MP4 and MKV | 4 |
| `features/13-audio-extraction.md` | mp3, opus, flac | 4 |
| `features/14-image-galleries.md` | image galleries | 4 |
| `features/15-google-drive.md` | Drive files and Docs exports | 4 |
| `features/16-drm-detection.md` | refusing protected media | 4 |
| `features/17-bit-torrent.md` | magnet links and `.torrent` | 6 |
| `features/18-mcp-server.md` | AI assistant access | 6 |
| `features/19-phone-remote.md` | phone on the LAN | 6 |
| `features/20-queue-scheduler.md` | overnight queue windows | 6 |
| `features/21-languages.md` | catalogues and RTL layouts | 7 |
