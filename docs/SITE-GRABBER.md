# Site Grabber

The Site Grabber reads one website, lists the files its pages link to, and adds
the ones you choose to the download list. It is optional: nothing runs until
you open **Tasks → Run site grabber** (`Ctrl` accelerators do not open it) and
press **Read site**.

A grabber is aimed at a site by address, which makes it the easiest tool in the
application to point somewhere it should not go. The rules below are therefore
part of the feature, not advice around it.

## Using it

1. Open **Tasks → Run site grabber**.
2. Enter the address of a page (not a file).
3. Choose the depth, the page limit and the file types. Depth 1 reads only that
   page; depth 2 also reads the pages it links to, and so on.
4. Press **Read site**. Pages are read one at a time, 300 ms apart, and the
   files appear in the list as they are found.
5. Untick anything you do not want, then press **Download N files**. Those
   files join the ordinary queue, categories and connection budget.

The crawl can be stopped at any time with **Stop**. Closing the window does not
stop it; reopening the window shows the crawl where it is.

## What it will not do

| Boundary | Rule |
| --- | --- |
| Scheme | Only `http://` and `https://`. No `file:`, `ftp:`, `javascript:` or `data:`. |
| Credentials | An address with a user name or password is refused, in the start field and in every link. |
| Scope | Only the start host and its `www.` twin are crawled. A subdomain or another site is not followed. |
| Private networks | `localhost`, single-label names, `.local`, `.internal`, `.home.arpa`, `.lan` and every IP literal in a private, loopback, link-local, carrier-grade NAT, documentation, multicast or reserved range are refused. Cloud metadata endpoints (`169.254.169.254`, `metadata.google.internal`, `100.100.100.200`, and their IPv6 spellings) are refused with them. |
| DNS | Every page and every redirect is resolved before it is requested, and the request is refused if **any** answer is a private address. This catches a public name that points at the router. |
| Redirects | Followed manually, one hop at a time, at most five. Each hop is re-checked, and a hop that leaves the site stops the page. |
| robots.txt | Fetched and obeyed for every page on the site. A `5xx`, a rate-limit answer or a redirect of `robots.txt` blocks the site rather than being read as permission. A `4xx` means "no usable file", which RFC 9309 allows. |
| Rate | 300 ms between pages, longer when the site asks for a `Crawl-delay` (capped at 5 s). A `429` stops the crawl. |
| Size | 2 MiB per page, 500 pages, depth 5, 2 000 files, 2 000 links read per page. A page over the limit is skipped with a line in the log. |
| Content | Only HTML is parsed. A PDF, an image or a stream is reported as "not a web page", not searched for links. |
| Authentication | No cookies, no sign-in, no paywall and no DRM are bypassed. A page that refuses the crawler (`401` / `403`) is reported and skipped. |
| Files | The crawl never downloads a file. A file is fetched only after you tick it and press download, through the same engine as any other download. |
| Destination | The folder comes from the system folder picker and must be absolute. A name that is taken becomes `name (2).ext`; a grabber never replaces a file. |

Every selected file is resolved again before its download starts, so a name
that has changed since the crawl is refused instead of sending the engine to a
private address. The engine resolves the name once more when it connects.

## Defaults

| Setting | Default | Range |
| --- | --- | --- |
| Follow links | 1 (this page only) | 1–5 |
| Pages to read | 50 | 1–500 |
| File types | Video, Music, Documents, Archives, Programs | any |
| Extra types | none | any extension |
| Connections | 8 per file | 1–32 |
| Queue | Start now | Start now / Main queue |

Images and unknown types are **off** by default: a gallery or a page of design
assets would otherwise fill the list with files nobody asked for. A file is
never ticked for you. This is what keeps "read a site" from becoming "download
a site" in one click.

## What counts as a file

The file type comes from the last segment of the link's path, never from a
query string. That is the only place a server reliably puts it, and guessing
from `?file=` would offer links that are pages.

These are never offered as files: pages and scripts (`html`, `php`, `js`,
`json`, `xml`, `css`), fonts, source maps, and stream manifests or segments
(`m3u8`, `mpd`, `m4s`, `ts`). The rest fall into the groups the form lists, and
an extension no group claims is **Other**.

## Reporting

Every page that is refused, every path you excluded, every repeated link and
every file the app would not start is a line in **Log**, with the address
without its query. The list header counts the files that were left out by
reason. The crawl says when the page or file limit ended it early.

Download answers mirror the browser hand-over: started, queued, skipped,
already in the list, invalid, or refused with the app's own sentence.

## Limitations

- **Rendered pages only, as sent.** Links added by JavaScript after the page
  loads are not seen. The crawler parses the HTML it receives.
- **No charset conversion.** A page in a legacy encoding is read as UTF-8, so
  non-ASCII link addresses may not resolve. Most link paths are ASCII.
- **No compressed pages.** The request asks for `identity`; a response that
  arrives gzip-compressed is reported instead of guessed at.
- **One site, one host.** Assets on a CDN are listed as files when they match a
  chosen type, but pages on another host (even a subdomain) are not crawled.
  `robots.txt` is checked for crawled pages, not for every host a file lives
  on.
- **No sitemap, no JavaScript, no forms.** The grabber follows anchors and
  media sources; it does not submit forms, log in, or read a page's script.
- **No crawl resume.** Stopping or closing ends that read. The files already
  found stay in the window; a new start begins from the address again.
- **DNS is checked, then used.** A name could change between the check and the
  request (DNS rebinding). The redirect checks make this a narrow window rather
  than an open door, but a hostile authoritative DNS server remains a
  theoretical risk.
- **Download redirects belong to the engine.** The crawl and `robots.txt`
  re-check every redirect hop. A selected file is checked when it is listed and
  again at hand-over, but the download engine then follows up to ten redirects
  on its own, as it does for every other download in the app. A public file
  that redirects to a private address at download time is therefore a residual
  risk shared with the add dialog, not a guarantee the grabber can make alone.
- **No concurrent crawl.** Pages are read one at a time, so the run is slow
  by design: one request in flight, a pause between pages. The connections
  setting applies to downloads, not to the crawl.
- **No per-site crawl delay beyond `robots.txt`.** The fixed 300 ms pause is
  polite for a personal grabber; it is not a search-engine budget.
- **Meta robots tags are not read.** `robots.txt` is, but a page's
  `<meta name="robots">` is not treated as a rule for the crawl.
- **Terms are yours.** `robots.txt` and rate limiting are respected, but a site
  can still forbid scraping in its terms. The grabber offers files; it does not
  grant permission to take them.

## Where it lives

| Layer | Files |
| --- | --- |
| Policy, names, rules | [`src-tauri/src/grabber/policy.rs`](../src-tauri/src/grabber/policy.rs) |
| robots.txt | [`src-tauri/src/grabber/robots.rs`](../src-tauri/src/grabber/robots.rs) |
| HTML links | [`src-tauri/src/grabber/extract.rs`](../src-tauri/src/grabber/extract.rs) |
| The walk | [`src-tauri/src/grabber/crawl.rs`](../src-tauri/src/grabber/crawl.rs) |
| Session, commands, hand-over | [`src-tauri/src/grabber/mod.rs`](../src-tauri/src/grabber/mod.rs) |
| Window | [`src/components/windows/grabber-window.tsx`](../src/components/windows/grabber-window.tsx) |
| Form, results, hook | [`src/components/windows/grabber/`](../src/components/windows/grabber/) |
| Contract and helpers | [`src/lib/grabber.ts`](../src/lib/grabber.ts) |

The download hand-over calls `actions::start`, the same function the add
dialog and the browser bridge use. The grabber has no download engine of its
own.

## Tests

- `cargo test --manifest-path src-tauri/Cargo.toml` — 130 tests, including the
  grabber's policy (private and metadata addresses, NAT64 and IPv4-mapped
  spellings, exclusions, file types), the robots parser (groups, wildcards,
  anchors, crawl delay), the link scanner (comments, scripts, entities,
  relative and protocol-relative links, `<base href>`, cut-off pages, the link
  cap), the request limits, and a crawl against a loopback server: depth,
  deduplication, exclusion, page limit, redirects on and off the site, a
  redirect to a metadata endpoint, cancellation, and robots refusals through
  the fetch paths.
- `pnpm check:grabber` — the window helpers: address wording, rule parsing,
  preference round-trip and clamping, event folding, snapshot merging,
  filtering, selection, chunking, the report and the log cap.

The integration tests use a test-only allowance for a server on the loopback
interface. Every other private destination still goes through the production
policy, so the tests can assert that a metadata endpoint is refused.
