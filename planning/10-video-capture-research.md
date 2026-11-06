# 10 - Video capture: how IDM works (research, no implementation)

> **Status note, 6 November 2026.** Two things about this file. (1) Section 6 is
> the plan written before the media path existed; that path has since been built
> in-house, and section 6 says exactly which parts of the plan shipped, which
> were reversed, and which still hold. Sections 2 to 5 and 7 are the
> reverse-engineering record and are unchanged. (2) The evidence here is
> **static** unless a line says otherwise. Nothing was observed at runtime; the
> binary was never executed.

This document is research and plan only. No code changed while writing it. The
goal: understand how IDM captures a YouTube video, how it builds the quality
list, how it joins audio and video, and what a plan to build the same thing in
this project would look like.

Every claim carries its evidence: a file, an address or a source path. Anything
not verified is listed in the unknowns section.

---

## 1. The questions

| # | Question | Answered? |
|---|---|---|
| 1 | How does a video URL get from the browser to IDM? | yes |
| 2 | Where does IDM get the quality list on YouTube? | yes |
| 3 | How are labels like "1080p HD, 60fps" produced? | yes |
| 4 | Audio and video arrive separately - how are they joined? | yes |
| 5 | How many connections, and how does resume work? | yes |
| 6 | What does it not do (DRM, 8K, AV1, live)? | yes |

---

## 2. Evidence: which files, and what came out of them

Everything came from the installed copy at
`C:\Program Files (x86)\Internet Download Manager\`, IDM 6.43.16.2.

| File | What it is (FileDescription) | What it gave |
|---|---|---|
| `IDMGCExt.crx` | Chrome extension, v6.43.1 | MV3 manifest, network sniffing, native messaging |
| `idmmzcc.xpi` | Firefox add-on | the same job, over the older XUL bridge |
| `IDMMsgHost.json` | native messaging config | host name `com.tonec.idm`, path `IDMMsgHost.exe` |
| `IDMMsgHost.exe` | "Native Messaging Host" | the pipe from the browser extension to IDM |
| `idmcchandler7_64.dll` | "click catcher for browsers" | URL/content-type dispatcher, YouTube `fmt_stream_map` parser, HLS parser |
| `IDMVMPrs64.dll` | "Parser Module" | **the real MPD/DASH parser** (`GetMPDParserInfo`, `qualityRanking`); it holds no HLS `EXT-X` string at all |
| `IDMan.exe` | main app | grabber UI, quality labels, merge, connections |
| `idmvconv.dll` | "IDM Module" | container muxer; verified strings: `MuxMP4AndMKVToMKV`, `MuxMKVAndMKVToMKV`, `MuxTSandTStoMKV`, `IDMmkvlib` |
| `idmbrbtn64.dll` | "Download Panel" | the floating panel |

REA (Ghidra 12.1.4) opened these three, with SHA-256 so the same build can be
re-checked later:

| File | SHA-256 |
|---|---|
| `idmcchandler7_64.dll` | `d464e8e7c84cb2fc62eacf932e841bbd73c3294a37812ccee7ffbbb9e01572a6` |
| `IDMVMPrs64.dll` | `6ff10a3025e0e74bbe9fd99ea21bdaa20a32c94e13145b8805acc82cf1229f5f` |
| `IDMan.exe` | `3b5dc9ce53eb31933effa3fd973bec068a78e0f8c1d15a919b081f0c4d9325ba` |

`IDMan.exe` is a stripped x86 binary, so the evidence from it is string-level
only (Ghidra's code analysis does not run on it).

---

## 3. IDM's YouTube flow, step by step

### Step 1 - the browser extension is only a sniffer

Inside `IDMGCExt.crx`, the `manifest.json` (verbatim):

```
manifest_version: 3
version: 6.43.1
minimum_chrome_version: 109.0
content_scripts: content.js, matches *://*/*, all_frames, document_start
web_accessible_resources: document.js, debug.js, captured.js, captured.html
permissions: scripting, tabs, cookies, contextMenus, webNavigation,
             webRequest, declarativeNetRequest, downloads, downloads.shelf,
             downloads.ui, management, storage, proxy, nativeMessaging
host_permissions: <all_urls>
```

The extension never mentions YouTube. It watches network traffic and decides
whether something is media from its MIME type. `background.js` holds the full
MIME -> extension table, and these are the entries in it:

```
video/mp4          -> MP4|M4V|M4S
video/webm         -> WEBM
audio/mp4          -> M4A|MP4|M4S
application/vnd.apple.mpegurl -> M3U8
application/dash+xml          -> MPD
video/vnd.mpeg.dash.mpd       -> MPD
application/vnd.yt-ump        -> UMP
```

The last line matters: `application/vnd.yt-ump` is YouTube's UMP, the DRM
stream. IDM detects it but cannot open it.

### Step 2 - the hooks inside the page (config-driven)

`document.js` runs in the page's main world, but its patch list is **not
hardcoded**: `content.js` first sends a config string through
`postMessage([1229212978, ...])` (split on `|`) carrying property paths and
regexes. Only after that config do these patches go in:

| What | How |
|---|---|
| XHR `open` / `send` | prototype methods wrapped; on a URL regex match a marker is added and the flag set |
| fetch | global replace, wrapper when the URL matches |
| `Response.json` / `.text` / `.arrayBuffer` / `.blob` / `.formData` | the body readers wrapped |
| `Response.body` getter | the stream wrapped (`stream: true`) |

Every extracted body is handed back to `content.js` with
`postMessage([1229212980, ...])`. So IDM is "listening" inside the page, but the
config says **what** to listen to.

Important: that config is not YouTube-specific. Across the whole extension
(`gc/*.js` and the manifest) there is not one occurrence of `youtube`,
`googlevideo`, `videoplayback`, `itag`, `ytInitial`, `player_response` or
`adaptiveFormats` - only `m3u8` (13 times in `background.js`). The extension
does not recognise YouTube at all; it is a generic sniffer, and YouTube's work
happens on the native side (steps 4-6).

### Step 3 - native messaging

The extension hands the URL, the headers and the **cookies** to
`IDMMsgHost.exe` (`IDMMsgHost.json`: `"name": "com.tonec.idm", "type": "stdio"`).

Why the cookies: the privacy policy in `welcome.html` says it itself - IDM
resends the very request the browser was making, with its URL, cookies, query
parameters and post data. That is why a download works on a logged-in site, and
that is also why it does not work on a DRM site (a DRM licence is a different
thing entirely).

### Step 4 - recognising YouTube

`idmcchandler7_64.dll` contains these strings (REA `search_strings`, real
addresses):

| Address | String |
|---|---|
| `0x18008c290` | `youtube.com/get_video_info?` |
| `0x18008c270` | `youtube.com/api_video_info?` |
| `0x18008c3c8` | `youtube.com/videoplayback?` |
| `0x180087c00` | `http://manifest.googlevideo.com/api/manifest/` |
| `0x180087c60` | `https://manifest.googlevideo.com/api/manifest/` |
| `0x1800a7078` | `.?AVCYoutubeParser@@` (RTTI) |
| `0x1800a7410` | `.?AVCYoutubeDashXMLParser@@` (RTTI) |
| `0x1800a7100` | `.?AVIDMSTRUCT_YOUTUBEDASHTRACK@CDownloadButtonsArray@@` |
| `0x1800a7150` | `.?AVIDMSTRUCT_YOUTUBEDASHMP4VIDEO@CDownloadButtonsArray@@` |

The `xrefs` show all of them leading into one procedure, `FUN_180045500`
(0x180045500, about 32 KB): `0x18004bcc1`, `0x18004bd9b`, `0x18004bcf7`,
`0x18004755c`, `0x18004758e`, `0x18004732b`, `0x18004be4b`. So the
URL/content-type **dispatch** is in one place; the real work is in separate
helpers - the itag table in `FUN_1800049a0`, the `fmt_stream_map` parser in
`FUN_180057450`, the HLS parser in `FUN_180051f80` plus `FUN_180052460` (steps
5, 6, 9).

### Step 5 - YouTube: two routes, both now verified

`FUN_180045500` (`0x180045500`, `idmcchandler7_64.dll`, 112,401 characters of
pseudocode) is the whole dispatcher: it looks at the URL and the content type
and decides what to do. It **does not make the HTTP request itself** - this DLL
contains no `WinHttp`, `HttpSendRequest`, `InternetOpenUrl`,
`MaxConnectionsNumber`, `IDMMsgHost` or `\\.\pipe\` string at all (REA
`search_strings`, every one `(none)`). The DLL only inspects; `IDMan.exe` does
the downloading, and the hand-off goes through COM (`@idm/mozilla_cc_helper5;1`).

The dispatcher's branches (literal `strstr` calls in the pseudocode):

| Match | What happens |
|---|---|
| `youtube.com/videoplayback?` / `googlevideo.com/videoplayback?` | `&itag=` is pulled out with `sscanf`, and the URL is used **exactly as it is** |
| `youtube.com/get_video_info?`, `youtube.com/api_video_info?`, `googlevideo.com/get_video_info?` | the answer is parsed when it is `application/json`; `FUN_18001fa10(url,"el","adunit")` filters ads |
| `youtube.com/watch?` + `&v=` | the page URL |
| `manifest.googlevideo.com/api/manifest/` (http and https) | its own branch |
| `.m3u8` | HLS |
| `player.vimeo.com/video`, `facebook.com/ajax/` | two more special cases |

Size caps on the content type (all literal constants):

| Test | Cap |
|---|---|
| `video/*` (and not `flash`) | `< 0x79d39` (499,001) and `!= 0` |
| `mpd`, `application/dash+xml` | `< 0x2dc6c1` (3,000,001) |
| `f4m`, `text+xml`, `application/vnd.lumberjack.manifest` | `< 0x186a1` (100,001) |
| `ram` | `< 0x800` (2,048) |

**Route A - the click catcher (what runs today).** The browser's own player
requests `googlevideo.com/videoplayback?...&itag=137...`; IDM captures that
signed URL and reuses it. No cipher is solved and the `n` parameter is not
touched (`&n=` and `?n=` are `(none)` in both binaries). The gain: it always
works. The cost: the YouTube throttle (`n`) stays exactly what the browser was
handed.

**Route B - the old `get_video_info` parse (the code is still in the binary).**
`FUN_180057450` matches both `url_encoded_fmt_stream_map` and `"adaptive_fmts":`,
takes `url=`, `itag=`, `sig=`, `s=` out of each entry (and the JSON-escaped
`\u0026itag=` / `\u0026s=` forms), removes duplicates with `FUN_180055890`, and
then registers on `FUN_180002a60(ctx,url,itag,isAdaptive)`. When the URL does not
already carry `signature=`, it appends the **raw** value:

```
pwVar12 = wcsstr(pOVar10,L"signature=");
if (pwVar12 == (wchar_t *)0x0) {
  ...
  pOVar7 = SysAllocString(L"&signature=");
  ...
  FUN_180003d90(&local_88,pOVar7,UVar4);   // URL + "&signature="
  thunk_FUN_180059f5f(pwVar5,0x2e);
  ...
}
```

That is where this route is weak: in the ciphered format `s` is an obfuscated
string that the player JS transforms. IDM appends the raw `s`, so such a format
comes back `403`. In other words IDM does **not** solve the cipher - it only
hopes the signature is already in the URL.

> YouTube switched `get_video_info` off in 2021. The strings are still in the
> binary today (a fallback, or for old embeds).

**DASH is not here.** This DLL holds no `SegmentTemplate`, `AdaptationSet`,
`ContentProtection`, `availabilityStartTime` or `availabilityEndTime`. The RTTI
name `CYoutubeDashXMLParser` exists (`0x1800a7410`) but its xrefs are empty, and
no MPD element string is in this DLL. The real MPD parser is `IDMVMPrs64.dll`
(step 10). So the claim "IDM runs YouTube from the DASH manifest" was wrong - it
only detects the manifest and passes it on.

### Step 6 - the quality list: the itag table (verified, `FUN_1800049a0`)

`FUN_1800049a0` is a `switch(itag)`; every branch sets a container (BSTR) and a
label. The complete table that decompiled:

| itag | Container | Label |
|---|---|---|
| 5 | flv | - |
| 17, 36 | 3gp | - |
| 18, 22, 37, 38 | mp4 | - |
| 43, 44, 45, 46 | webm | - |
| 82, 83, 84, 85 | mp4 | 3D labels |
| 100, 101, 102 | webm | 3D labels |
| 133 | mp4 | 240p |
| 134 | mp4 | 360p |
| 135 | mp4 | 480p |
| 136 | mp4 | 720p HD |
| 137 | mp4 | 1080p HD |
| 138 | mp4 | Ultra HD |
| 160 | mp4 | 144p |
| 171, 172 | MKV | - |
| 242 | MKV | 240p |
| 243 | MKV | 360p |
| 244, 245, 246 | MKV | 480p |
| 247 | MKV | 720p HD |
| 248 | MKV | 1080p HD |
| 264 | mp4 | 1080p HD, high bitrate |
| 266 | mp4 | 2160p 4K |
| 271 | MKV | 1440p HD |
| 272, 313 | MKV | 2160p 4K |
| 278 | MKV | 144p |
| 298 | mp4 | 720p HD, 60fps |
| 299 | mp4 | 1080p HD, 60fps |
| 302 | MKV | 720p HD, 60fps |
| 303 | MKV | 1080p HD, 60fps |
| 308 | MKV | 1440p HD, 60fps |
| 315 | MKV | 2160p K, 60fps |

`2160p K, 60fps` is IDM's own typo (a "K" where it means "4K").

The labels are built at runtime; these format strings sit around
`0x1800877d0`:

```
%s %s, %s %s        (container/codec + resolution + flags)
, %ld kbps          (bitrate)
, %s %ldp           (e.g. "MP4 480p")
, %s %ldp HD        (e.g. "MP4 1080p HD")
Ultra HD 4K
```

So a label is container/codec + resolution + HD + fps + bitrate, which is why
IDM shows text like "MP4 1080p HD, 60fps".

**The limits that fall out of this same table (verified):**

- The highest entry is 2160p (4K). **No 8K / 4320p label exists.**
- AV1 (itag 399/400) is not in the table. The RTTI has
  `IDMSTRUCT_YOUTUBEDASHMP4VIDEO`, but there is no AV1 itag branch.
- Audio-only itags (139, 140, 251) are **not** in the table - the audio track
  comes from the DASH/adaptive list, not from this table. So the table covers
  video itags (and the old muxed ones) only.
- VP9 cannot be put into MP4, which is why there is an MKV fallback. The binary
  even carries the help link for it:
  `https://www.internetdownloadmanager.com/support/mkvvp9codec.html`

### Step 7 - the download

In `IDMan.exe`:

```
MaxConnectionsNumber
Optional connections have been created.
Optional connections not needed.
%I64d
&range=
```

So IDM splits one file across several byte ranges, and adds or drops "optional
connections" as the load looks - dynamic splitting. That is exactly what this
project already has in `crates/idm-core/src/segmenter.rs`.

### Step 8 - joining audio and video (merge)

Above 1080p YouTube serves audio and video apart. IDM downloads both, then
**remuxes** them - it does not re-encode.

`IDMan.exe` contains a whole muxer under the namespace `ts2mp4`:

```
AacCodec, Ac3Codec, Eac3Codec, H264Codec, HevcCodec, MpegAudioCodec
AudioStream, VideoStream, EsStream, Fmp4Source, MkvSource, WebmSource
CMp4Converter, CMp4DashReader
-tmp_dashinit
Convert to mp4, s1
Convert to mp4, s2
%s.mp4.tmp
```

`-tmp_dashinit` means: the DASH **init segment** (`ftyp` + `moov`) is written
separately, and the media segments follow it. That is the classic fMP4 remux.

`idmvconv.dll` is the container-level muxer:

```
MuxMP4AndMKVToMKV, MuxMKVAndMKVToMKV, MuxTSandTStoMKV
IDMmkvlib0.1
```

So:

| Input | Output |
|---|---|
| MP4 video + M4A/MP4 audio | `.mp4` |
| VP9/Opus (WebM) | `.mkv` |
| TS (HLS) | `.mp4` or `.mkv` |

### Step 9 - the HLS parser (verified, `FUN_180052460` + `FUN_180051f80`)

HLS is in two functions, both in `idmcchandler7_64.dll`.

**Master playlist - `FUN_180051f80`** (`#EXT-X-STREAM-INF` xref `0x1800520d5`,
`#EXT-X-MEDIA` xref `0x18005204b`):

| Tag | Behaviour |
|---|---|
| `#EXTM3U` | first line, flag `ctx+0xc` |
| `#EXT-X-VERSION:` | `sscanf %d` -> `ctx+0x58` |
| `#EXT-X-MEDIA:` | its own record, appended to the list at `ctx+8` |
| `#EXT-X-STREAM-INF:` | a stream entry; the next non-`#` line is its URI |
| `RESOLUTION=`, `BANDWIDTH=`, `CODECS=`, `NAME=`, `SUBTITLES=` | parsed as attributes |

**What the master parser does not do:** no `FRAME-RATE`, `AVERAGE-BANDWIDTH`,
`AUDIO=` or `CHANNELS` string exists in the DLL. So a separate audio rendition
group (`#EXT-X-MEDIA:TYPE=AUDIO` + `AUDIO="..."`) is never paired, and the fps
label never comes from the master playlist.

**Media playlist - `FUN_180052460`:**

| Tag | Behaviour |
|---|---|
| `#EXTM3U` | flag `ctx+0xc` |
| `#EXT-X-VERSION:` | `ctx+0x58` |
| `#EXT-X-TARGETDURATION:`, `#EXT-X-MEDIA-SEQUENCE:` | parsed |
| `#EXT-X-PLAYLIST-TYPE:` | matched, **value ignored** |
| `#EXT-X-BYTERANGE:` | parsed |
| `#EXT-X-DISCONTINUITY`, `#EXT-X-PROGRAM-DATE-TIME` | parsed |
| `#EXTINF:` | segment duration |
| `#EXT-X-MAP:` | a duration-0 segment; flagged `ctx+0xd8` unless it is `.ts` |
| `#EXT-X-KEY:` | **only sets a flag**; METHOD/URI/IV are not parsed |
| `EXT-X-ENDLIST` | not in this function; its handler is `0x1800354ae` (not opened in this research) |

Encryption: nowhere in the DLL is there an `AES`, `decrypt`, `IV=`, `METHOD=` or
`EXT-X-SESSION-KEY` string. So on encrypted HLS IDM neither decrypts nor reports
a clear error - the file is written and is unplayable. (The
`#EXT-X-STREAM-INF` and `#EXT-X-MEDIA` strings do exist, but in the master
parser, not the media parser.)

### Step 10 - the DASH parser (verified, `IDMVMPrs64.dll`)

The real MPD parser is `IDMVMPrs64.dll` (`GetMPDParserInfo`). The elements that
exist as strings:

```
MPD   Period   AdaptationSet   Representation   SubRepresentation
SegmentTemplate   SegmentBase   RepresentationIndex   ContentProtection
```

The attributes parsed: `bandwidth`, `minBandwidth`, `maxBandwidth`, `frameRate`,
`minFrameRate`, `maxFrameRate`, `codecs`, `qualityRanking`, `audioSamplingRate`,
`AudioChannelConfiguration`, `startWithSAP`, `maxPlayoutRate`,
`codingDependency`, `scanType`.

**Live DASH is parsed:** `MPD@type` is compared against `OnDemand`, `static`,
`Live` and `dynamic`; on `Live`/`dynamic` the flag `_Dst+6 = 1` is set. The live
attributes are parsed too: `availabilityStartTime`, `availabilityEndTime`,
`mediaPresentationDuration`, `minimumUpdatePeriod`, `minBufferTime`,
`timeShiftBufferDepth`, `suggestedPresentationDelay`, `maxSegmentDuration`,
`maxSubsegmentDuration`.

**DRM:** `ContentProtection` is only recorded in the representation struct
(field `+0xf`). Neither refused nor decrypted. No `Widevine`, `DRM` or
`encrypted` string is in this DLL.

There is no `sidx` string either - the `SegmentBase` index comes from the
`RepresentationIndex` element, not from the box name.

### The whole flow, in one block

```
browser plays
  -> the extension watches webRequest (video/*, audio/*, m3u8, mpd, yt-ump)
  -> native messaging: URL + headers + cookies
  -> the click catcher matches URL + content type
     - videoplayback?&itag=NN -> the same signed URL goes on the queue (route A)
     - get_video_info / adaptive_fmts -> itag + sig parsed, &signature= appended
       (route B, no cipher solved)
     - mpd / dash+xml -> IDMVMPrs64.dll parses the MPD (static and dynamic)
     - m3u8 -> master first, then the media playlist
  -> every format = one quality button (itag table + label format strings)
  -> the user picks a quality
  -> video-only + audio-only, both as multi-connection range downloads
  -> init segment + media segments remuxed (.mp4, or .mkv for VP9)
```

---

## 4. What does not work (with the proof)

| Case | Verified reason |
|---|---|
| 8K / AV1 | no itag 399/400 in the table, no 4320p label |
| DRM (UMP, `application/vnd.yt-ump`) | the content type is detected; no `Widevine`, `DRM` or `encrypted` string in either parser DLL (click catcher or MPD parser) |
| `ContentProtection` in an MPD | only recorded (struct `+0xf`), never refused - the file downloads and is unplayable |
| Encrypted HLS (AES-128) | `#EXT-X-KEY` only sets a flag; no `AES` / `decrypt` / `IV=` anywhere in the DLL - an unplayable file and no clear error |
| Ciphered YouTube formats | the raw `&signature=<s>` is appended, never transformed - `403` |
| The `n` throttle | `&n=` / `?n=` are absent from both binaries - the throttle is inherited from the browser |
| HLS separate audio | no `AUDIO=` / `CHANNELS` string - the audio rendition is never paired |
| HLS `FRAME-RATE` | no such string in the master parser - no fps label |
| Live streams | parsed (the `dynamic`/`Live` flag), but the recording behaviour was never observed at runtime |
| A logged-in site (Udemy and the like) | cookies are forwarded, but a URL built inside the player JS can still be missed - this one is inference, not measurement |

---

## 5. Comparison with AB Download Manager (open source, code read)

Repos: `D:\Dev\ab-download-manager` (commit `286e2a4`) and
`D:\Dev\ab-download-manager-browser-integration` (commit `658a0c7`). Kotlin
Multiplatform + Compose.

| Thing | IDM | AB Download Manager |
|---|---|---|
| Extension sniffing | `webRequest` + a MIME table | `LinkGrabber`, `MediaRegistry`, `OnMediaInterceptedFromRequestListener` |
| HLS | yes - master (`FUN_180051f80`) + media (`FUN_180052460`), TS | yes - `hls-parser`, `HLSDownloadJob`, `HLSPartDownloader` |
| HLS audio rendition | no (no `AUDIO=` string) | no |
| DASH / MPD | yes - `IDMVMPrs64.dll`, static + dynamic/live | **no** (nothing in the code handles `dash` or `mpd`) |
| YouTube | yes - click-catcher URL reuse + the legacy `fmt_stream_map`; no cipher, no `n` | **no** |
| Encrypted HLS | `#EXT-X-KEY` only sets a flag, no decrypt, unplayable output | refused in words: `"Encrypted HLS playlists are not supported"` (`HLSResponseInfo.kt:67`) |
| Multi-connection | `MaxConnectionsNumber` + optional connections | `PartSplitSupport` + `SplitToRange(size, minPartSize, maxPartCount)` + `resumeSupport` |
| Remux | `ts2mp4` + `idmvconv` | **no** - plain byte concat |
| Speed limit | yes | `SpeedLimiter`, `OkioCustomizedThrottler` |
| Queues / scheduler | yes | `DownloadQueue`, `QueueManager`, `ScheduleTimes` |

**AB DM's HLS merge (verified, `HLSDownloadJob.kt` +
`SegmentedDownloadDestination.kt`):** no remux at all. Every segment goes into
its own file (`tempDirectory/<partID>`), and
`SegmentedDownloadDestination.assemble` concatenates them byte by byte in
part-id order:

```kotlin
sources.forEach { sourceFile ->
    sourceFile.inputStream().use { src ->
        while (true) { val len = src.read(buffer); if (len == -1) break; dst.write(buffer, 0, len) }
    }
}
```

`HLSResponseInfo.fromConnection` accepts only a leading `.ts` segment (`"Only
HLS .ts segments supported at the moment..."`) and refuses an encrypted playlist
at line 65. So after the byte concat the file is still `.ts`; wanting `.mp4`
means a remux is unavoidable.

**The lesson:** AB Download Manager is a clean, modern, multi-connection
downloader - exactly what this project is. It took one step into video (HLS) and
left DASH and YouTube. So HLS is the first step, DASH the second, YouTube the
third.

Other open-source options (their code was not verified in this session; listed
as references only): `aria2` (multi-connection engine, no video), `Motrix` (an
aria2 GUI), `yt-dlp` (the real reference for YouTube extraction), `N_m3u8DL-RE`
(HLS/DASH + merge), `ffmpeg` (the standard remux tool).

---

## 6. The plan for this project (plan only)

> **Measured again on 6 November 2026.** This was the plan written before the
> media path existed, and its opening line - "the range engine is there, media
> splitting is not" - has not been true for some time. The order it argued for
> is the order that was followed, so the section is kept as written; these are
> the places where the product parted company with the plan:
>
> - **Phases A and B shipped.** `media/hls.rs` and `media/dash.rs` parse the
>   playlists, `media/download.rs` fetches every segment through the ordinary
>   range engine and resumes from the segments already on disk, and the panel
>   lists the formats the manifest really offers.
> - **Phase A point 3 shipped the way IDM does not.** The audio rendition is
>   paired with its variant (`Master::audio_group`,
>   `quality.rs:380 an_hls_master_is_paired_with_its_audio_group`), and
>   `FRAME-RATE` is read as well (`hls.rs:8`) - the two things IDM's parser
>   cannot do, which is why it shows no fps label and never pairs separate
>   audio.
> - **Phase A point 7 shipped the way AB DM does it:** encrypted HLS is refused
>   with a clear sentence instead of producing an unplayable file
>   (`refuse_key`, `hls.rs:389`, tested by
>   `an_encrypted_playlist_is_refused_with_one_sentence`, with
>   `METHOD=NONE` correctly read as "not encrypted").
> - **Phase C went the expensive way.** The native remuxer in
>   `src-tauri/src/video/mp4.rs` and `mux.rs` replaced `ffmpeg` entirely
>   (`ADR-0005-native-remuxer`,
>   [14](./14-own-youtube-and-native-muxer-plan.md)); the bundle no longer
>   carries an external tool, which is what made the cheap route unacceptable.
> - **Phase D point 4 is dead.** The `yt-dlp` sidecar was tried, shipped in
>   0.3.x, and then deleted. YouTube extraction is now in-house against the
>   InnerTube iOS client (`src-tauri/src/video/yt.rs`,
>   `ADR-0006-youtube-ios-innertube`), which returns directly playable tracks
>   instead of a ciphered URL.
> - **Phase D point 3 still holds and is still the rule:** no signature solver,
>   no `n` solver. That line has not moved.

At the time of writing, `crates/idm-core` had the HTTP range engine and no media
splitting. The order of work should be:

### Phase A - HLS (the easiest, the biggest gain)

1. Detect `.m3u8` (from the extension or the URL).
2. Master playlist -> variants (`BANDWIDTH`, `RESOLUTION`, `CODECS`,
   `FRAME-RATE`), shown to the user as a list.
3. Parse `#EXT-X-MEDIA:TYPE=AUDIO` groups too and pair each variant with its
   audio rendition through `AUDIO="..."`. IDM does not do this (step 9) - we
   should.
4. Media playlist -> segments (`#EXTINF`, the `#EXT-X-MAP` init segment).
5. Download the segments in parallel through the existing range engine.
6. Concatenate the segments in order -> `.ts`, then remux to `.mp4`.
7. Reject AES-128 (`#EXT-X-KEY`) for now, with a clear error - the way AB DM
   does it.

### Phase B - DASH (MPD)

1. Parse the MPD XML: `Period` -> `AdaptationSet` -> `Representation`.
2. Build the quality list from each `Representation`'s `id`, `bandwidth`,
   `width`x`height` and `codecs`.
3. Expand the `SegmentTemplate` (`$Number$`, `$Time$`, `$RepresentationID$`).
4. Download video-only and audio-only apart, then remux.

### Phase C - Remux (without ffmpeg, or with it)

Two routes:

- **Cheap:** `ffmpeg -i v -i a -c copy out.mp4`. One dependency, works
  completely.
- **Expensive:** write our own fMP4 writer. IDM did exactly this (`ts2mp4`, 30+
  classes).

The lazy rule said: ffmpeg first, own muxer only if bundling ffmpeg is not
acceptable. Bundling it turned out to be the thing worth avoiding - the
dependency is gone and the in-house writer is what ships.

### Phase D - YouTube (last, and the most fragile)

1. Default: **reuse the very signed URL** the player itself asked for (IDM's
   route A, step 5). No signature, no `n`, nothing to solve - which is why it
   never "breaks". The price: the throttle stays.
2. Fallback: the formats from the page's player response
   (`ytInitialPlayerResponse`) or `/youtubei/v1/player` - but **only** when their
   URLs are already signed. Sticking the raw `s` onto `&signature=` is the bug
   that makes IDM's route B return 403.
3. No signature / `n` solver of our own. It changes every week - a maintenance
   black hole.
4. If it still does not work: use `yt-dlp` as a sidecar.

### What should never enter this project

- DRM bypass (UMP, Widevine). Out of bounds legally and technically.
- A signature cipher solver of our own. A maintenance black hole.

---

## 7. Unknowns - what is left

What was unknown before is now resolved:

| Thing | Status now |
|---|---|
| The `n=` throttle parameter | **resolved** - it is not handled (`&n=` / `?n=` are `(none)` in both binaries) |
| Does IDM solve the cipher | **resolved** - no. It appends the raw `s` with `&signature=` (`FUN_180057450`) |
| Which process does the download | **resolved** - there is no HTTP client in the DLL; a COM hand-off goes to `IDMan.exe` |
| Live DASH support | **resolved** - `dynamic`/`Live` is parsed, the flag is set, and the live attributes are parsed |
| AB DM's HLS concat | **resolved** - plain byte concat, no remux |
| The YouTube DASH manifest path | **resolved** - no MPD parser in `idmcchandler7_64.dll`; `IDMVMPrs64.dll` is the generic MPD parser |

The gaps that genuinely remain, plainly:

| Gap | Why it remains |
|---|---|
| Runtime packet capture (running IDM against live YouTube) | not done in this session. Everything here is static analysis (REA/Ghidra). So "does IDM call `get_video_info` today" has no answer: **the code exists, whether it is reached is unknown** |
| Live DASH recording behaviour | the parsing is verified; the recording path was never watched running |
| Missing a player-JS URL on a logged-in site | inference, not measurement |
| The size of the `n` throttle | the throttle is inherited, but how slow it makes the stream was never measured |

If anyone takes this feature further: the first row above (runtime capture)
should come first, because everything else is confirmed through it.

---

## 8. Legal

YouTube's Terms of Service do not allow third-party downloaders. This document
exists only to understand how an installed piece of software works, so that our
own downloader is built the clean way. No DRM bypass, no signature
circumvention.
