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
| `idmcchandler7_64.dll` | "click catcher for browsers" | URL/content-type dispatcher, YouTube `fmt_stream_map` parser, HLS parser |
| `IDMVMPrs64.dll` | "Parser Module" | **asli MPD/DASH parser** (`GetMPDParserInfo`, `qualityRanking`); ismein HLS ka koi `EXT-X` string nahi |
| `IDMan.exe` | main app | grabber UI, quality labels, merge, connections |
| `idmvconv.dll` | "IDM Module" | container muxer; verified strings: `MuxMP4AndMKVToMKV`, `MuxMKVAndMKVToMKV`, `MuxTSandTStoMKV`, `IDMmkvlib` |
| `idmbrbtn64.dll` | "Download Panel" | floating panel |

REA (Ghidra 12.1.4) se ye teen khule, SHA-256 ke saath (isi build par dobara
verify karne ke liye):

| File | SHA-256 |
|---|---|
| `idmcchandler7_64.dll` | `d464e8e7c84cb2fc62eacf932e841bbd73c3294a37812ccee7ffbbb9e01572a6` |
| `IDMVMPrs64.dll` | `6ff10a3025e0e74bbe9fd99ea21bdaa20a32c94e13145b8805acc82cf1229f5f` |
| `IDMan.exe` | `3b5dc9ce53eb31933effa3fd973bec068a78e0f8c1d15a919b081f0c4d9325ba` |

`IDMan.exe` stripped x86 hai, is liye wahan sirf string-level evidence hai
(Ghidra code analysis nahi chalti).

---

## 3. IDM ka YouTube flow, step by step

### Step 1 - Browser extension sirf "sniffer" hai

`IDMGCExt.crx` ke andar `manifest.json` (exact):

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

### Step 2 - Page ke andar hook (config-driven)

`document.js` page ke main world me chalta hai, lekin uska patch list **hardcoded
nahi**: `content.js` pehle `postMessage([1229212978, ...])` se config string
bhejta hai (`|` se split), jisme property paths aur regex aate hain. Us config ke
baad hi ye patch lagte hain:

| Kya | Kaise |
|---|---|
| XHR `open` / `send` | prototype methods wrap, URL regex match par marker add aur flag |
| fetch | global replace, URL match par wrapper |
| `Response.json` / `.text` / `.arrayBuffer` / `.blob` / `.formData` | body readers wrap |
| `Response.body` getter | stream wrap (`stream: true`) |

Har extracted body `postMessage([1229212980, ...])` se `content.js` ko jati hai.
Yani IDM page ke andar "sun raha" hota hai, lekin **kya** sunna hai wo config
batati hai.

Ahem: ye config YouTube-specific **nahi** hai. Poore extension (`gc/*.js` aur
manifest) me `youtube`, `googlevideo`, `videoplayback`, `itag`, `ytInitial`,
`player_response`, `adaptiveFormats` ka ek bhi string nahi - sirf `m3u8` hai
(`background.js` me 13 dafa). Yani extension YouTube ko pehchanta hi nahi; wo
generic sniffer hai, aur YouTube ka kaam native side (Step 4-6) karta hai.

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

`xrefs` se ye sab ek hi procedure `FUN_180045500` (0x180045500, ~32 KB) me jaate
hain: `0x18004bcc1`, `0x18004bd9b`, `0x18004bcf7`, `0x18004755c`, `0x18004758e`,
`0x18004732b`, `0x18004be4b`. Yani URL/content-type **dispatch** ek jagah hai;
asli kaam alag helpers me hai - itag table `FUN_1800049a0`, `fmt_stream_map`
parser `FUN_180057450`, HLS parser `FUN_180051f80` + `FUN_180052460`
(Step 5, 6, 9).

### Step 5 - YouTube: do raaste, dono ab verified

`FUN_180045500` (`0x180045500`, `idmcchandler7_64.dll`, 112,401 characters
pseudocode) poora dispatcher hai: URL aur content-type dekh kar decide karta hai
ke kya karna hai. Wo **khud HTTP request nahi karta** - is DLL me `WinHttp`,
`HttpSendRequest`, `InternetOpenUrl`, `MaxConnectionsNumber`, `IDMMsgHost` aur
`\\.\pipe\` ka koi string nahi (REA `search_strings`, sab `(none)`). Yani DLL
sirf dekhti hai, download `IDMan.exe` karta hai, aur handoff COM se hota hai
(`@idm/mozilla_cc_helper5;1`).

Dispatcher ke branches (pseudocode me literal `strstr`):

| Match | Kya hota hai |
|---|---|
| `youtube.com/videoplayback?` / `googlevideo.com/videoplayback?` | `&itag=` `sscanf` se nikala jata hai, URL **jaisa hai waisa** use hota hai |
| `youtube.com/get_video_info?`, `youtube.com/api_video_info?`, `googlevideo.com/get_video_info?` | jawab `application/json` ho to parse; `FUN_18001fa10(url,"el","adunit")` se ad filter |
| `youtube.com/watch?` + `&v=` | page URL |
| `manifest.googlevideo.com/api/manifest/` (http aur https) | alag branch |
| `.m3u8` | HLS |
| `player.vimeo.com/video`, `facebook.com/ajax/` | do aur special cases |

Content-type par size caps (sab literal constants):

| Test | Cap |
|---|---|
| `video/*` (aur `flash` nahi) | `< 0x79d39` (499,001) aur `!= 0` |
| `mpd`, `application/dash+xml` | `< 0x2dc6c1` (3,000,001) |
| `f4m`, `text+xml`, `application/vnd.lumberjack.manifest` | `< 0x186a1` (100,001) |
| `ram` | `< 0x800` (2,048) |

**Raasta A - click catcher (jo aaj chalta hai).** Browser ka player khud
`googlevideo.com/videoplayback?...&itag=137...` maangta hai; IDM wahi signed URL
pakad kar dobara use karta hai. Isme na cipher solve hota hai, na `n` parameter
chhua jata hai (`&n=` aur `?n=` dono binaries me `(none)`). Faida: hamesha
chalta hai. Nuksan: YouTube ki throttle (`n`) wahi rehti hai jo browser ko mili
thi.

**Raasta B - purana `get_video_info` parse (code me aaj bhi hai).**
`FUN_180057450` `url_encoded_fmt_stream_map` aur `"adaptive_fmts":` dono match
karta hai, har entry se `url=`, `itag=`, `sig=`, `s=` (aur JSON-escaped
`\u0026itag=` / `\u0026s=` forms) nikalta hai, duplicate `FUN_180055890` se
hataata hai, phir `FUN_180002a60(ctx,url,itag,isAdaptive)` par register karta
hai. Agar URL me `signature=` pehle se nahi hai to wo **raw** value append karta
hai:

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

Yahi is raaste ki kamzori hai: ciphered format me `s` ek obfuscated string hota
hai jise player JS transform karta hai. IDM raw `s` append kar deta hai, is liye
aisa format 403 deta hai. Yani IDM cipher **solve nahi** karta - wo sirf umeed
karta hai ke signature pehle se URL me maujood ho.

> `get_video_info` YouTube ne 2021 me band kar diya. Strings aaj bhi binary me
> hain (fallback / purane embed ke liye).

**DASH yahan nahi hai.** Is DLL me `SegmentTemplate`, `AdaptationSet`,
`ContentProtection`, `availabilityStartTime`, `availabilityEndTime` - kuch bhi
nahi. `CYoutubeDashXMLParser` ka RTTI naam maujood hai (`0x1800a7410`), lekin
uske xrefs khali hain aur MPD ka koi element string is DLL me nahi. Asli MPD
parser `IDMVMPrs64.dll` hai (Step 10). Yani "IDM DASH manifest se YouTube
chalata hai" wali baat galat thi - wo sirf detect karta hai aur aage bhejta hai.

### Step 6 - Quality list: itag table (verified, `FUN_1800049a0`)

`FUN_1800049a0` ek `switch(itag)` hai; har branch container (BSTR) aur label
set karta hai. Poora table jo decompile hua:

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

`2160p K, 60fps` IDM ki apni typo hai ("4K" ki jagah "K").

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

**Limits jo isi table se nikalte hain (verify kiye):**

- Sabse oonchi entry 2160p (4K) hai. **8K / 4320p label maujood nahi.**
- AV1 (itag 399/400) table me nahi hai. RTTI me `IDMSTRUCT_YOUTUBEDASHMP4VIDEO`
  hai lekin AV1 ka koi itag branch nahi.
- Audio-only itags (139, 140, 251) table me **nahi** hain - audio track
  DASH/adaptive list se aata hai, is table se nahi. Yani ye table sirf video
  (aur purane muxed) itags ke liye hai.
- VP9 ko MP4 me nahi daal sakta - is liye MKV fallback hai. Binary me is kaam ka
  help link bhi hai:
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

### Step 9 - HLS parser (verified, `FUN_180052460` + `FUN_180051f80`)

HLS do functions me hai, dono `idmcchandler7_64.dll` me:

**Master playlist - `FUN_180051f80`** (`#EXT-X-STREAM-INF` xref
`0x1800520d5`, `#EXT-X-MEDIA` xref `0x18005204b`):

| Tag | Behaviour |
|---|---|
| `#EXTM3U` | pehli line, flag `ctx+0xc` |
| `#EXT-X-VERSION:` | `sscanf %d` -> `ctx+0x58` |
| `#EXT-X-MEDIA:` | alag record bana kar `ctx+8` list me |
| `#EXT-X-STREAM-INF:` | stream entry, agli non-`#` line uska URI |
| `RESOLUTION=`, `BANDWIDTH=`, `CODECS=`, `NAME=`, `SUBTITLES=` | attributes parse hote hain |

**Jo master parser nahi karta:** `FRAME-RATE`, `AVERAGE-BANDWIDTH`, `AUDIO=`,
`CHANNELS` ka koi string DLL me nahi. Yani separate audio rendition group
(`#EXT-X-MEDIA:TYPE=AUDIO` + `AUDIO="..."`) pair nahi hota, aur fps label
master playlist se nahi aata.

**Media playlist - `FUN_180052460`:**

| Tag | Behaviour |
|---|---|
| `#EXTM3U` | flag `ctx+0xc` |
| `#EXT-X-VERSION:` | `ctx+0x58` |
| `#EXT-X-TARGETDURATION:`, `#EXT-X-MEDIA-SEQUENCE:` | parse |
| `#EXT-X-PLAYLIST-TYPE:` | match hota hai, **value ignore** |
| `#EXT-X-BYTERANGE:` | parse |
| `#EXT-X-DISCONTINUITY`, `#EXT-X-PROGRAM-DATE-TIME` | parse |
| `#EXTINF:` | segment duration |
| `#EXT-X-MAP:` | duration-0 segment; `.ts` na ho to flag `ctx+0xd8` |
| `#EXT-X-KEY:` | **sirf ek flag set**, METHOD/URI/IV parse nahi |
| `EXT-X-ENDLIST` | is function me nahi; uska handler `0x1800354ae` hai (is research me detail nahi kholi) |

Encryption: poore DLL me `AES`, `decrypt`, `IV=`, `METHOD=`,
`EXT-X-SESSION-KEY` ka koi string nahi. Is liye encrypted HLS par IDM na
decrypt karta hai, na saaf error deta hai - file banti hai aur unplayable hoti
hai. (`#EXT-X-STREAM-INF` aur `#EXT-X-MEDIA` strings maujood hain, lekin wo
master parser me hain, media parser me nahi.)

### Step 10 - DASH parser (verified, `IDMVMPrs64.dll`)

MPD ka asli parser `IDMVMPrs64.dll` hai (`GetMPDParserInfo`). Jo elements
strings ki tarah maujood hain:

```
MPD   Period   AdaptationSet   Representation   SubRepresentation
SegmentTemplate   SegmentBase   RepresentationIndex   ContentProtection
```

Attributes jo parse hote hain: `bandwidth`, `minBandwidth`, `maxBandwidth`,
`frameRate`, `minFrameRate`, `maxFrameRate`, `codecs`, `qualityRanking`,
`audioSamplingRate`, `AudioChannelConfiguration`, `startWithSAP`,
`maxPlayoutRate`, `codingDependency`, `scanType`.

**Live DASH parse hota hai:** `MPD@type` ko `OnDemand`, `static`, `Live` aur
`dynamic` se compare kiya jata hai; `Live`/`dynamic` par flag `_Dst+6 = 1` set
hota hai. Live attributes bhi parse hote hain: `availabilityStartTime`,
`availabilityEndTime`, `mediaPresentationDuration`, `minimumUpdatePeriod`,
`minBufferTime`, `timeShiftBufferDepth`, `suggestedPresentationDelay`,
`maxSegmentDuration`, `maxSubsegmentDuration`.

**DRM:** `ContentProtection` sirf representation struct me record hota hai
(field `+0xf`). Na refuse, na decrypt. `Widevine`, `DRM`, `encrypted` strings
is DLL me nahi hain.

`sidx` string bhi nahi hai - SegmentBase ka index `RepresentationIndex` element
se aata hai, box ke naam se nahi.

### Poora flow, ek line me

```
browser play
  -> extension webRequest dekhta hai (video/*, audio/*, m3u8, mpd, yt-ump)
  -> native messaging: URL + headers + cookies
  -> click catcher URL + content-type match karta hai
     - videoplayback?&itag=NN -> wahi signed URL queue me (Raasta A)
     - get_video_info / adaptive_fmts -> itag + sig parse, &signature= append
       (Raasta B, cipher solve nahi)
     - mpd / dash+xml -> IDMVMPrs64.dll MPD parse (static aur dynamic dono)
     - m3u8 -> master, phir media playlist parse
  -> har format = ek quality button (itag table + label format strings)
  -> user quality chunta hai
  -> video-only + audio-only, dono multi-connection range download
  -> init segment + media segments remux (.mp4, VP9 ho to .mkv)
```

---

## 4. Kya kaam nahi karta (saboot ke saath)

| Case | Verified wajah |
|---|---|
| 8K / AV1 | itag table me 399/400 nahi, 4320p label nahi |
| DRM (UMP, `application/vnd.yt-ump`) | content-type detect hota hai; `Widevine`, `DRM`, `encrypted` strings in dono parser DLLs (click catcher + MPD parser) me nahi |
| MPD me `ContentProtection` | sirf record hota hai (struct `+0xf`), refuse nahi hota - file download ho kar unplayable banti hai |
| HLS encrypted (AES-128) | `#EXT-X-KEY` sirf flag set karta hai; `AES` / `decrypt` / `IV=` poore DLL me nahi - unplayable file, saaf error nahi |
| YouTube ciphered formats | raw `&signature=<s>` append hota hai, transform nahi - 403 |
| `n` throttle | `&n=` / `?n=` dono binaries me nahi - throttle browser se inherit hoti hai |
| HLS separate audio | `AUDIO=` / `CHANNELS` strings nahi - audio rendition pair nahi hota |
| HLS `FRAME-RATE` | master parser me string nahi - fps label nahi milta |
| Live stream | parsed hota hai (`dynamic`/`Live` flag), lekin recording behaviour runtime par verify nahi hua |
| Login wali site (Udemy etc.) | cookies jaate hain, lekin player JS ke andar bana URL miss ho sakta hai (ye inference hai) |

---

## 5. AB Download Manager se comparison (open source, code parha)

Repo: `D:\Dev\ab-download-manager` (commit `286e2a4`) aur
`D:\Dev\ab-download-manager-browser-integration` (commit `658a0c7`).
Kotlin Multiplatform + Compose.

| Cheez | IDM | AB Download Manager |
|---|---|---|
| Extension sniffing | `webRequest` + MIME table | `LinkGrabber`, `MediaRegistry`, `OnMediaInterceptedFromRequestListener` |
| HLS | haan - master (`FUN_180051f80`) + media (`FUN_180052460`), TS | haan - `hls-parser`, `HLSDownloadJob`, `HLSPartDownloader` |
| HLS audio rendition | nahi (`AUDIO=` string nahi) | nahi |
| DASH / MPD | haan - `IDMVMPrs64.dll`, static + dynamic/live | **nahi** (code me `dash`/`mpd` ka koi handle nahi) |
| YouTube | haan - click-catcher URL reuse + legacy `fmt_stream_map`; cipher/`n` nahi | **nahi** |
| Encrypted HLS | `#EXT-X-KEY` sirf flag, decrypt nahi, unplayable | saaf mana: `"Encrypted HLS playlists are not supported"` (`HLSResponseInfo.kt:67`) |
| Multi-connection | `MaxConnectionsNumber` + optional connections | `PartSplitSupport` + `SplitToRange(size, minPartSize, maxPartCount)` + `resumeSupport` |
| Remux | `ts2mp4` + `idmvconv` | **nahi** - plain byte concat |
| Speed limit | haan | `SpeedLimiter`, `OkioCustomizedThrottler` |
| Queues / scheduler | haan | `DownloadQueue`, `QueueManager`, `ScheduleTimes` |

**AB DM ka HLS merge (verified, `HLSDownloadJob.kt` + `SegmentedDownloadDestination.kt`):**
koi remux nahi. Har segment apni file me jata hai (`tempDirectory/<partID>`),
phir `SegmentedDownloadDestination.assemble` unko part id ki tarteeb se
byte-by-byte concat karta hai:

```kotlin
sources.forEach { sourceFile ->
    sourceFile.inputStream().use { src ->
        while (true) { val len = src.read(buffer); if (len == -1) break; dst.write(buffer, 0, len) }
    }
}
```

`HLSResponseInfo.fromConnection` sirf `.ts` pehla segment accept karta hai
(`"Only HLS .ts segments supported at the moment..."`) aur encrypted playlist ko
line 65 par refuse karta hai. Yani byte concat ke baad file `.ts` hi rehti hai;
`.mp4` chahiye to remux lazmi hai.

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
2. Master playlist -> variants (`BANDWIDTH`, `RESOLUTION`, `CODECS`,
   `FRAME-RATE`), user ko list dikhao.
3. `#EXT-X-MEDIA:TYPE=AUDIO` groups bhi parse karo aur `AUDIO="..."` se variant
   ka audio rendition jodo. IDM ye nahi karta (Step 9) - humein karna chahiye.
4. Media playlist -> segments (`#EXTINF`, `#EXT-X-MAP` init segment).
5. Segments ko mojooda range engine se parallel download karo.
6. Segments ko tarteeb se concat karo -> `.ts`, phir `.mp4` remux.
7. AES-128 (`#EXT-X-KEY`) abhi **reject** karo, saaf error do - AB DM ki tarah.

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

1. Default: **wahi signed URL reuse** jo player ne khud maanga (IDM ka Raasta A,
   Step 5). Ismein na signature, na `n` - kuch solve nahi karna padta, is liye
   ye kabhi "toota" nahi. Qeemat: throttle wahi rehti hai.
2. Fallback: page ka player response (`ytInitialPlayerResponse`) ya
   `/youtubei/v1/player` se formats - **sirf** jab unke URL pehle se signed hon.
   Raw `s` ko `&signature=` ke saath chipkana bug hai (IDM ka Raasta B isi wajah
   se 403 deta hai).
3. Signature / `n` ka apna solver **nahi**. Ye har hafte badalta hai - maintenance
   ka black hole.
4. Agar phir bhi na chale: `yt-dlp` ko sidecar ki tarah use karo.

### Jo cheez is project me daalni hi nahi

- DRM bypass (UMP, Widevine). Ye legal aur technical dono taur par out.
- Signature cipher ka apna solver. Maintenance ka black hole.

---

## 7. Unknowns - ab kya bacha hai

Pehle jo "unknown" tha, wo ab resolve ho gaya:

| Cheez | Ab status |
|---|---|
| `n=` throttle param | **resolved** - handle nahi hota (`&n=` / `?n=` dono binaries me `(none)`) |
| Kya IDM cipher solve karta hai | **resolved** - nahi. Raw `s` ko `&signature=` ke saath append karta hai (`FUN_180057450`) |
| IDM kis process me download karta hai | **resolved** - DLL me HTTP client nahi, COM handoff -> `IDMan.exe` |
| Live DASH support | **resolved** - `dynamic`/`Live` parse hota hai, flag set hota hai, live attributes bhi parse hote hain |
| AB DM HLS concat | **resolved** - plain byte concat, koi remux nahi |
| YouTube DASH manifest path | **resolved** - `idmcchandler7_64.dll` me MPD parser nahi; `IDMVMPrs64.dll` generic MPD parser hai |

Bacha hua asli gap, saaf lafzon me:

| Cheez | Kyun bacha |
|---|---|
| Runtime packet capture (IDM ko live YouTube par chalana) | is session me nahi kiya. Sab kuch static analysis (REA/Ghidra) se hai. Isi liye "aaj IDM `get_video_info` call karta hai ya nahi" ka jawab **code maujood hai, call hota hai ya nahi pata nahi** hai |
| Live DASH ki recording behaviour | parse karna verified hai; recording chal kar dekhi nahi |
| Login wali sites par player-JS URL miss hona | inference hai, measured nahi |
| `n` throttle ka asar | throttle inherit hoti hai, magar kitni slow hoti hai - measured nahi |

Agar aage koi ye feature implement kare: upar wali pehli row (runtime capture)
sab se pehle karni chahiye, kyunki baqi sab usi se confirm hota hai.

---

## 8. Legal

YouTube ki Terms of Service third-party downloaders ko allow nahi karti. Ye
document sirf ye samajhne ke liye hai ke installed software kaise kaam karta
hai, taake apna downloader saaf tareeqe se bane. Koi DRM bypass nahi, koi
signature circumvention nahi.
