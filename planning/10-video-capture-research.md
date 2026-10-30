# 10 - Video capture: IDM kaise kaam karta hai (research, koi implementation nahi)

Ye document sirf research aur plan hai. Isme koi code change nahi hua. Maqsad:
samajhna ke IDM YouTube video kaise pakadta hai, quality kaise banata hai, audio
aur video ko kaise jodta hai, aur is project me isko kabhi banane ka plan kya
hoga.

Har claim ke saath evidence likha hai: file, address ya source path. Jo cheez
verify nahi hui, wo "unknown" section me hai.

---

## 1. Sawaal

| # | Sawaal | Jawab mila? |
|---|---|---|
| 1 | Browser se video URL IDM tak kaise pahunchta hai? | haan |
| 2 | YouTube par IDM ko quality list kahan se milti hai? | haan |
| 3 | "1080p HD, 60fps" jaise labels kaise bante hain? | haan |
| 4 | Audio aur video alag aate hain, jode kaise jaate hain? | haan |
| 5 | Kitni connections, resume kaise? | haan |
| 6 | Kya nahi hota (DRM, 8K, AV1, live)? | haan |

---

## 2. Evidence: kaunsi files aur kya mila

Sab kuch installed copy se, `C:\Program Files (x86)\Internet Download Manager\`,
IDM 6.43.16.2.

| File | Kya hai (FileDescription) | Isse kya mila |
|---|---|---|
| `IDMGCExt.crx` | Chrome extension, v6.43.1 | MV3 manifest, network sniffing, native messaging |
| `idmmzcc.xpi` | Firefox add-on | wahi kaam, purana XUL bridge |
| `IDMMsgHost.json` | native messaging config | host name `com.tonec.idm`, path `IDMMsgHost.exe` |
| `IDMMsgHost.exe` | "Native Messaging Host" | browser extension se IDM tak pipe |
| `idmcchandler7_64.dll` | "click catcher for browsers" | **asli YouTube parser** |
| `IDMVMPrs64.dll` | "Parser Module" | generic MPD/HLS parser (`GetMPDParserInfo`, `qualityRanking`) |
| `IDMan.exe` | main app | grabber UI, quality labels, merge, connections |
| `idmvconv.dll` | "IDM Module" | container muxer (MP4/MKV/FLV/TS) |
| `idmbrbtn64.dll` | "Download Panel" | floating panel |

REA (Ghidra 12.1.4) se `idmcchandler7_64.dll` khula,
SHA-256 `d464e8e7c84cb2fc62eacf932e841bbd73c3294a37812ccee7ffbbb9e01572a6`.

---

## 3. IDM ka YouTube flow, step by step

### Step 1 - Browser extension sirf "sniffer" hai

`IDMGCExt.crx` ke andar `manifest.json`:

```
permissions: scripting, tabs, cookies, contextMenus, webNavigation,
             webRequest, declarativeNetRequest, downloads, proxy,
             nativeMessaging
host_permissions: <all_urls>
```

Extension ke andar YouTube ka koi zikr nahi. Wo sirf network traffic dekhta hai
aur MIME type se decide karta hai ke ye media hai ya nahi. `background.js` me
poora MIME -> extension table hai, jisme ye entries hain:

```
video/mp4          -> MP4|M4V|M4S
video/webm         -> WEBM
audio/mp4          -> M4A|MP4|M4S
application/vnd.apple.mpegurl -> M3U8
application/dash+xml          -> MPD
video/vnd.mpeg.dash.mpd       -> MPD
application/vnd.yt-ump        -> UMP
```

Aakhri line ahem hai: `application/vnd.yt-ump` YouTube ka UMP hai (DRM wala
stream). IDM usko detect karta hai, par khol nahi sakta.

### Step 2 - Page ke andar hook

`document.js` page ke MAIN world me inject hota hai aur monkey-patch karta hai:

- `window.fetch`
- `XMLHttpRequest.prototype.open` / `send`
- `Response.json` / `Response.text` / `Response.arrayBuffer`
- `JSON.stringify`

Phir response body ko `postMessage` se `content.js` ko bhejta hai. Yani IDM
browser ke andar "sun raha" hota hai ke page kaun sa JSON le raha hai -
YouTube ka player response bhi isi tarah pakda jata hai.

### Step 3 - Native messaging

Extension URL + headers + **cookies** `IDMMsgHost.exe` ko deta hai
(`IDMMsgHost.json`: `"name": "com.tonec.idm", "type": "stdio"`).

Cookies kyun: `welcome.html` ki privacy policy khud kehti hai ke IDM wahi
request dobara bhejta hai jo browser bhej raha tha - URL, cookies, query
parameters, post data. Isi liye login wali site par bhi download chalta hai,
aur isi liye DRM wali site par nahi chalta (DRM licence alag cheez hai).

### Step 4 - YouTube ko pehchanna

`idmcchandler7_64.dll` me ye strings hain (REA `search_strings`, asli
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

`xrefs` se `youtube.com/get_video_info?` aur `googlevideo.com/videoplayback?`
dono ek hi procedure `FUN_180045500` (0x180045500, ~32 KB) me jaate hain. Yani
poora YouTube handling ek hi bade parser me hai.

### Step 5 - Do raaste (purana aur naya)

**Purana raasta (2010s):**

`IDMan.exe` me ye strings hain:

```
url_encoded_fmt_stream_map
"adaptive_fmts":
itag=
&itag=
&sig=
&signature=
```

Yani `get_video_info` ka URL-encoded jawab parse karo, `itag` + `sig` nikalo,
`googlevideo.com/videoplayback?...&itag=..&range=..` download karo.

**Naya raasta (DASH):**

`manifest.googlevideo.com/api/manifest/dash/id/<videoId>?...` se MPD milta hai,
usko `CYoutubeDashXMLParser` parse karta hai aur do structs banate hain:

- `IDMSTRUCT_YOUTUBEDASHMP4VIDEO` - video track
- `IDMSTRUCT_YOUTUBEDASHTRACK` - audio/aux track

MPD ke har `Representation` ka `id` hi itag hota hai, is liye itag alag se
guess nahi karna padta. Generic MPD/HLS parsing `IDMVMPrs64.dll` me hai
(`GetMPDParserInfo`, `qualityRanking`).

> Note: `get_video_info` YouTube ne 2021 me band kar diya. Strings aaj bhi
> binary me hain (purane sites / fallback ke liye), lekin asli kaam DASH
> manifest se hota hai. Ye inference hai, runtime par verify nahi kiya.

### Step 6 - Quality list banana

`idmcchandler7_64.dll` me quality labels ka poora set hai:

```
144p   240p   240p 3D   360p   360p 3D   480p
720p HD   720p HD 3D   720p HD, 60fps
1080p HD   1080p HD 3D   1080p HD, 60fps   1080p HD, high bitrate
1440p HD   1440p HD, 60fps
2160p 4K   2160p 4K, 60fps   2160p K, 60fps
```

Aakhri wala `2160p K, 60fps` IDM ki apni typo hai - saboot ke ye labels
hardcoded format strings hain, koi AI/normalisation nahi.

Labels runtime par bante hain, ye format strings `0x1800877d0` ke aas paas hain:

```
%s %s, %s %s        (container/codec + resolution + flags)
, %ld kbps          (bitrate)
, %s %ldp           (e.g. "MP4 480p")
, %s %ldp HD        (e.g. "MP4 1080p HD")
Ultra HD 4K
```

Yani label = container/codec + resolution + HD + fps + bitrate. Isi liye IDM
me "MP4 1080p HD, 60fps" jaisa text dikhta hai.

**Limits jo isi table se nikalte hain:**

- Sabse oonchi entry 2160p (4K) hai. **8K / 4320p nahi.**
- AV1 ka koi label nahi. Yani IDM AV1 (itag 399/400) offer nahi karta.
- VP9 hai, lekin VP9 ko MP4 me nahi daal sakta - is liye MKV fallback hai.
  Binary me is kaam ka help link bhi hai:
  `https://www.internetdownloadmanager.com/support/mkvvp9codec.html`

### Step 7 - Download

`IDMan.exe` me:

```
MaxConnectionsNumber
Optional connections have been created.
Optional connections not needed.
%I64d
&range=
```

Yani IDM ek hi file ko kai byte-ranges me todta hai, aur load dekh kar
"optional connections" khud add/remove karta hai (dynamic splitting). Ye wahi
cheez hai jo is project ke `crates/idm-core/segmenter.rs` me already hai.

### Step 8 - Audio + video jodna (merge)

YouTube 1080p+ par audio aur video alag deta hai. IDM do download karta hai,
phir **re-encode nahi**, sirf **remux** karta hai.

`IDMan.exe` ke andar poora muxer hai, namespace `ts2mp4`:

```
AacCodec, Ac3Codec, Eac3Codec, H264Codec, HevcCodec, MpegAudioCodec
AudioStream, VideoStream, EsStream, Fmp4Source, MkvSource, WebmSource
CMp4Converter, CMp4DashReader
-tmp_dashinit
Convert to mp4, s1
Convert to mp4, s2
%s.mp4.tmp
```

`-tmp_dashinit` ka matlab: DASH ka **init segment** (`ftyp` + `moov`) alag
likha jata hai, phir media segments uske peeche. Ye fMP4 remux ka classic
tareeqa hai.

`idmvconv.dll` container-level muxer hai:

```
MuxMP4AndMKVToMKV, MuxMKVAndMKVToMKV, MuxTSandTStoMKV
IDMmkvlib0.1
```

Yani:

| Input | Output |
|---|---|
| MP4 video + M4A/MP4 audio | `.mp4` |
| VP9/Opus (WebM) | `.mkv` |
| TS (HLS) | `.mp4` ya `.mkv` |

### Poora flow, ek line me

```
browser play
  -> extension webRequest dekhta hai (video/*, audio/*, m3u8, mpd, yt-ump)
  -> native messaging: URL + headers + cookies
  -> IDM decide karta hai: YouTube? (host match)
  -> DASH manifest mangao, MPD parse karo (CYoutubeDashXMLParser)
  -> har Representation = ek quality button (label format strings se)
  -> user quality chunta hai
  -> video-only + audio-only, dono multi-connection range download
  -> init segment + media segments remux (.mp4, VP9 ho to .mkv)
```

---

## 4. Kya kaam nahi karta (jo log shikayat karte hain)

| Case | Kyun |
|---|---|
| 8K / AV1 | label table 2160p par khatam, AV1 entry nahi |
| DRM (UMP, `application/vnd.yt-ump`) | detect hota hai, decrypt nahi hota |
| Live stream | DASH live manifest ke liye koi evidence nahi mila |
| Login wali site (Udemy etc.) | cookies jaate hain, lekin player JS ke andar bana URL miss ho sakta hai |
| HLS encrypted (AES-128) | IDM ke HLS path me key handling ka koi string nahi mila |

---

## 5. AB Download Manager se comparison (open source, code parha)

Repo: `D:\Dev\ab-download-manager` (commit `286e2a4`) aur
`D:\Dev\ab-download-manager-browser-integration` (commit `658a0c7`).
Kotlin Multiplatform + Compose.

| Cheez | IDM | AB Download Manager |
|---|---|---|
| Extension sniffing | `webRequest` + MIME table | `LinkGrabber`, `MediaRegistry`, `OnMediaInterceptedFromRequestListener` |
| HLS | haan (TS) | haan - `hls-parser`, `HLSDownloadJob`, `HLSPartDownloader` |
| DASH / MPD | haan (`CYoutubeDashXMLParser`) | **nahi** (code me `dash`/`mpd` ka koi handle nahi) |
| YouTube | haan (DASH manifest + itag) | **nahi** |
| Encrypted HLS | unknown | saaf mana: `"Encrypted HLS playlists are not supported"` (`HLSResponseInfo.kt:67`) |
| Multi-connection | `MaxConnectionsNumber` + optional connections | `PartSplitSupport` + `SplitToRange(size, minPartSize, maxPartCount)` + `resumeSupport` |
| Remux | `ts2mp4` + `idmvconv` | nahi (segments seedha jodta hai) |
| Speed limit | haan | `SpeedLimiter`, `OkioCustomizedThrottler` |
| Queues / scheduler | haan | `DownloadQueue`, `QueueManager`, `ScheduleTimes` |

**Sabak:** AB Download Manager ek saaf, modern, multi-connection downloader hai
- bilkul wahi cheez jo ye project ban raha hai. Usne video ki taraf qadam
rakha (HLS) lekin DASH/YouTube chhoda. Yani HLS pehla step hai, DASH doosra,
aur YouTube teesra.

Doosre open-source options (is session me code verify nahi kiya, sirf
reference): `aria2` (multi-connection engine, koi video nahi), `Motrix`
(aria2 ka GUI), `yt-dlp` (YouTube extraction ka asli reference), `N_m3u8DL-RE`
(HLS/DASH + merge), `ffmpeg` (remux ka standard tool).

---

## 6. Is project ke liye plan (sirf plan)

Abhi `crates/idm-core` me HTTP range engine hai, media splitting nahi
(`planning/README.md` bhi yahi kehta hai). Tarteeb ye honi chahiye:

### Phase A - HLS (sabse aasan, sabse zyada faida)

1. `.m3u8` detect karo (extension ya URL se).
2. Master playlist -> variants (`BANDWIDTH`, `RESOLUTION`), user ko list dikhao.
3. Media playlist -> segments (`#EXTINF`, `#EXT-X-MAP` init segment).
4. Segments ko mojooda range engine se parallel download karo.
5. Segments ko tarteeb se concat karo -> `.ts`, phir `.mp4` remux.
6. AES-128 (`#EXT-X-KEY`) abhi **reject** karo, saaf error do - AB DM ki tarah.

### Phase B - DASH (MPD)

1. MPD XML parse: `Period` -> `AdaptationSet` -> `Representation`.
2. `Representation` ke `id`, `bandwidth`, `width`x`height`, `codecs` se quality list.
3. `SegmentTemplate` (`$Number$`, `$Time$`, `$RepresentationID$`) expand karo.
4. Video-only + audio-only alag download, phir remux.

### Phase C - Remux (ffmpeg ke bagair, ya ffmpeg ke saath)

Do raaste:

- **Sasta:** `ffmpeg -i v -i a -c copy out.mp4`. Ek dependency, 100% kaam.
- **Mehnga:** apna fMP4 writer. IDM ne yahi kiya (`ts2mp4`, 30+ classes).

Ponytail rule: pehle ffmpeg. Apna muxer sirf tab jab ffmpeg bundle na karna ho.

### Phase D - YouTube (sabse aakhir, sabse nazuk)

1. Page ka player response (`ytInitialPlayerResponse`) ya `/youtubei/v1/player`
   se formats nikalo - yahi tareeqa `yt-dlp` use karta hai.
2. Signature / `n` throttle param ko solve karna padta hai (player JS chalta
   hai). Ye har hafte badalta hai - is liye ye feature **hamesha tootega**.
3. Isi liye mashwara: YouTube ke liye `yt-dlp` ko sidecar ki tarah use karo,
   khud implement na karo.

### Jo cheez is project me daalni hi nahi

- DRM bypass (UMP, Widevine). Ye legal aur technical dono taur par out.
- Signature cipher ka apna solver. Maintenance ka black hole.

---

## 7. Unknowns (jo verify nahi hua)

| Cheez | Kyun unknown |
|---|---|
| Kya IDM aaj `get_video_info` call karta hai? | strings hain, runtime traffic capture nahi kiya |
| `n=` throttle param IDM kaise handle karta hai | binary me `n=` ka koi obvious handler nahi mila |
| IDM kis process me download karta hai (browser plugin ya IDMan.exe) | `idmcchandler7` parser rakhta hai, lekin downloader main app lagta hai |
| IDM live DASH support karta hai ya nahi | koi live-specific string nahi mila |
| AB DM ka HLS concat kaise karta hai (byte concat ya remux) | `HLSDownloadJob.kt` poora nahi parha |

---

## 8. Legal

YouTube ki Terms of Service third-party downloaders ko allow nahi karti. Ye
document sirf ye samajhne ke liye hai ke installed software kaise kaam karta
hai, taake apna downloader saaf tareeqe se bane. Koi DRM bypass nahi, koi
signature circumvention nahi.
