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

- ✅ Per-host learned connection caps: remember what a host actually allows and
  cap the next download to it. `host-caps.json` aside, the Properties window
  has a button that forgets what the host allowed, so a changed network is
  not capped forever.
- ✅ Slow-connection shedding: drop a range that is far behind and give its
  bytes to a faster connection (`pace::stalled`, four hand-overs per run at most).
- ✅ Connection rebalancing until the last byte (`pace::lagging`).
- ✅ Server location per download: country code and RTT, shown in the status
  tab. No probe of loopback or local hosts.
- ✅ Freeze detector beside the crash monitor: a suspended machine is not
  called frozen (sendmessage with a timeout), a stuck one is reported once.
  The monitor writes a freeze report with a thread table and the stacks
  (`freeze-report.md`, `freeze-stacks.dmp`), and the About page shows it on
  the next start.

## Wave 3 — the wire

- ✅ Encrypted DNS: DoH, DoT, NextDNS profile. `crates/idm-core/src/net/dns.rs`
  holds the choice and the rules, `crates/idm-core/src/net/resolver.rs` asks it
  (hickory, over TLS or HTTPS, with the system's verifier). The engine builds
  one resolver when the settings are applied and every client it builds looks
  names up through it. Settings → Connections has the mode, one field per
  mode, the visible fallback and a test button.
- ✅ Network adapter binding with kill switch: the reader chooses a connection
  (a VPN, usually) and every client leaves through it. The choice is kept by
  the adapter's identity, so a renamed connection is still the one that was
  chosen. The socket is bound to the adapter's address and its interface
  (`IP_UNICAST_IF`), the system is watched (`NotifyIpInterfaceChange`), and
  when the adapter goes away the running downloads stop with one sentence
  instead of taking the normal route. The status bar shows the adapter's
  state. Windows is the system it was built for; the enumeration is empty on
  Linux and macOS yet.
- ✅ PAC proxy execution: the reader points at a proxy auto-config script, the
  application fetches it (with the same DNS and adapter as everything else),
  and the script decides per address which proxy a download uses — the way a
  browser decides it. The route it names is the one the download takes, and
  `DIRECT` means straight to the server.
- 🔧 The system's own auto-proxy setting (WinHTTP's configuration) is not read
  yet: only a script the reader points at is.

## Wave 4 — media

- ✅ Soft subtitles into MP4 (`mov_text`) and MKV (`S_TEXT/UTF8`):
  `media/subtitles.rs` parses the renditions, the manifest carries them
  (`Manifest::subtitles`), the quality panel lists them, and an FFmpeg built
  beside the tracks writes them into the container — WebVTT converts to
  SRT, timings are never shifted. A local caption file wins over a
  rendition.
- ✅ Audio extraction / conversion to mp3, opus, flac: the panel offers the
  original track or a conversion; the track is written out with its own
  extension, or FFmpeg re-writes it to MP3/Opus/FLAC — never the picture,
  never a re-encode of video.
- ✅ Image galleries: `media/gallery.rs` reads a page, resolves its images,
  dedupes and caps them (500), and the add dialog offers a download of every
  image into a folder named after the page, one ordinary resumable download
  per picture.
- ✅ Google Drive / Docs links: `sites/gdrive.rs` resolves file, export and
  editor links; the add dialog offers the export the document supports and
  probes the endpoint the bytes come from. A private file and an over-quota
  owner are named, never downloaded as an error page. A shared folder is
  refused with a sentence, not a download of a listing page.
- ✅ DRM detection: one place (`media/drm.rs`) reads `#EXT-X-KEY`, DASH
  `ContentProtection`, `vnd.yt-ump` and encrypted init segments, refuses with
  a reader's sentence naming the scheme, and never decrypts anything.

## Wave 5 — the outside world

- ✅ Debrid accounts (Real-Debrid, AllDebrid, Premiumize): `idm-debrid` is a
  dependency of the app, the accounts live in the OS credential store, the
  settings page shows only the account's name and its paid days, host links
  are unlocked before the start, and rate limits are respected. An
  "unlock this link" switch in the add dialog asks for this one, and the
  automatic switch in Settings → Services unlocks matching links.
  The generated URL never reaches the log or the downloads index.
- ✅ Archive handling, the rest: a finished archive unpacks itself when the
  setting asks, into a folder of its own name; the archive is deleted only
  after a clean extract with no refused entries; an encrypted archive holds
  the job and asks for its password in the completion window (twice, then
  sent to another tool); progress counts files while it unpacks; the outcome
  window says files, bytes, skipped and refused. 7z and RAR go through
  7-Zip when it is installed.

## Also in MDM, not in your list

Kept here so nothing is lost. Each is a crate MDM has and this project lacks:

- ✅ BitTorrent (magnet links, `.torrent`, seeding) — `crates/idm-torrent`
  speaks the peer protocol itself; the add dialog accepts a magnet link or
  a `.torrent` file; the torrent window shows files, peers and pieces; the
  piece order is the plain order, so a film plays while it comes in; seeding
  is off by default and asks before the upload stops.
- ✅ MCP server for AI assistants — `crates/idm-mcp`, a separate program
  speaking MCP over stdio, shipped beside the app (`externalBin`). The
  bridge answers its routes only while the Advanced page lets it, deletes a
  file only on the second explicit call, and the assistant is refused the
  rest while the door is closed.
- ✅ Phone / LAN remote control — a second listener, off by default, bound to
  one local interface, refusing every request whose source or `Host` header
  names something else. Pairing is a one-time code, the token is stored as a
  hash, and a device is revocable in Settings → Services. The tray can
  switch the whole server off.
- ✅ Queue scheduler — days of the week, a start and stop minute, an action
  at the end (keep running, pause the queue, power the computer off after a
  separate explicit confirmation and only when every download is saved).
  The app checks the window on a clock, so an app that opens mid-window
  starts the queue. Missed windows do not fire at noon; the status bar says
  when the queue runs next.
- 🔧 86 languages and right-to-left layouts — the window's chrome (the
  settings window, the menus' shell, the status bar) reads in English,
  French, German and Arabic, and Arabic mirrors it. The rest of the
  interface is still English; the catalogue (`src/i18n/`) and the checker
  (`scripts/check-i18n.mjs`) hold the door open.
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
  the next download from it. Shown in the progress window's connection details,
  and the host cap can be forgotten from the Properties window.
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

## Wave 3, done

- ✅ **Encrypted DNS** — the reader chooses how names are resolved: the
  system's, DNS over HTTPS, DNS over TLS or a NextDNS profile. The choice
  lives in the engine's settings file (`dns` in `settings.json`), the engine
  builds one resolver when the settings are applied, and every client it
  builds — downloads, the grabber, video tracks, the update check — looks
  names up through it (`idm_core::net::client_builder`). The names a public
  server cannot know stay with the system's resolver; the chosen one may
  fall back only when the reader allows it, and only when it could not be
  reached. Settings → Connections has the mode, one field per mode, the
  fallback switch and a button that asks one name through the resolver and
  says how long it took.
  - The reader's line for the release notes: "Names can now be looked up
    through an encrypted resolver, so the sites you download from cannot see
    the names you ask for."
- ✅ **Adapter binding with a kill switch** — the reader chooses the network
  adapter every download leaves through, usually a VPN. The socket is bound
  to it (address and interface), the system is watched, and when the adapter
  goes away the downloads stop rather than quietly taking the normal route.
  The status bar shows the adapter, so a lost one is seen and not inferred.
  - The reader's line for the release notes: "Downloads can be held to one
    network adapter, so they stop when that adapter goes away instead of
    taking another route."
- ✅ **PAC proxy** — a proxy auto-config script decides, for every address,
  whether a download goes through a proxy or straight to the server. The
  script is fetched with the same DNS and adapter as everything else, and
  the answer is asked per address, the way a browser asks it.
  - The reader's line for the release notes: "A proxy auto-config script now
    decides, for each address, whether a download goes through a proxy."

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
