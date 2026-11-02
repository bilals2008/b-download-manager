# ADR-0005 — Pair the browser bridge with a code the reader copies

Status: accepted

## Context

The Firefox and Chrome builds talk to the application through a hand-rolled
HTTP/1.1 server on `127.0.0.1:27500`. Before this decision the only things that
separated the extension from every other program on the machine were an
`Origin` prefix test and a constant `X-BDM-Client: browser-extension` header.
Neither is a secret: any local process can send both, an `Origin` of
`moz-extension://anything` passed the prefix rule, and a request with no
`Origin` at all was admitted. That meant anything running as the user could list
downloads, pause or cancel them, start a download of any URL it liked into the
browser's folder, and raise the application's window.

The bridge architecture itself is not the problem and is not changed here: one
request per connection, no dependencies, fixed port, versioned JSON envelope.
What had to change is how a request is trusted.

## Decision

Pair, with a code the reader moves by hand, and keep every route as it was.

- The application makes 32 random bytes (`getrandom`, the operating system's
  source), writes them as lower-case hex, and keeps them in
  `browser-pairing.json` in its own data folder — a file of their own, never in
  `settings.json`, so a settings export or a support file does not carry a
  secret. A first run makes the code during setup, before the listener binds, so
  the bridge has something to compare against for its very first request.
- Every route but `GET /v1/ping` needs `Authorization: Bearer <code>`. The
  comparison runs over the whole string without returning early, and an empty
  stored code (no random source) refuses everything it should protect instead of
  falling back to a guessable value.
- The order of the checks is fixed: `Host` must name the bridge itself, then an
  `Origin` if present must be a well-formed `moz-extension://` or
  `chrome-extension://` identifier with no path or spaces, then the client
  header, then the code, then a POST body must be `application/json`. An
  `OPTIONS` preflight — which cannot carry either custom header — is let through
  once its origin and host are known.
- One code belongs to one extension. The first origin to present the code is
  pinned in the pairing file (trust on first use, because the pairing is the
  reader's own act of pasting), and a second extension carrying the same secret
  is refused with 403. **Make a new code** in Settings → Browser integration
  replaces the secret and clears the pin at once.
- `Access-Control-Allow-Origin` echoes only an extension origin, so a page that
  guesses the port gets neither a useful answer nor permission to read one.
- Refusals are refusals. 401 `unauthorized`, 403 `forbidden` /
  `forbidden_extension`, 415 for a body that is not JSON, and the extension maps
  401 to its own "not paired" state instead of reporting defaults as the app's
  answer.

The rules live in `src-tauri/src/bridge/auth.rs` as pure functions over one
`Request`, and the request-to-verdict step lives in `bridge::gate`, so the whole
matrix is tested without opening a socket. `scripts/bridge-probe.mjs` is the
same matrix over a live socket, with one real hand-over whose bytes are read
back off the disk.

## Consequences

- A one-time step appears in the setup path, and the app has to be honest about
  it: Settings names the code, its file, and which extension is paired; the
  popup and the options page distinguish "not running", "not responding", "not
  paired" and "another extension holds the pairing".
- Every hand-over that used to work without a code stops working until the
  reader pairs once. Both builds read the code from their own storage, so a
  reload of a temporary add-on keeps it and a new profile does not.
- A page cannot drive the bridge, and a local program cannot either unless it
  knows the code.
- **What this does not buy.** A process running as the same user can read the
  pairing file, so this is not a defence against an already-compromised account;
  the audit's "out of scope" line about a modified local build still applies. It
  also does not stop a code the reader pasted into the wrong extension — that is
  what the origin pin and *Make a new code* are for.
- A reconnect after a rotation is a refusal until the new code is pasted. The
  pin is stored, so it survives a restart of either side; nothing else about the
  contract changed, so an older extension build does not half-work — its
  hand-overs are refused until it is updated and paired.

## Alternatives rejected

- **Keep the header as the check.** It is a constant in two public repositories;
  it filters accidents, it does not authenticate.
- **Native messaging** (the reference product's route). Gives a real per-process
  identity and no port, but needs a manifest in a fixed OS location, a registry
  key on Windows, and a second executable with its own framing and lifetime —
  against a 3.3 MB installer and the ADR-0004 rule about not shipping extra
  binaries. Kept as the recorded alternative in the extension README, not
  implemented.
- **A WebSocket or server-sent-events channel** for pushing settings to open
  tabs. A push path would keep the video button in step with no polling at all,
  but it replaces a one-request-one-answer server with connection state, and the
  task was to preserve the bridge. The `alarms` read is once a minute, only while
  a tab has a video on it.
- **The system credential store** (Windows Credential Manager, keychain). Better
  at rest than a file, but it adds a per-OS dependency, an interactive prompt on
  a headless session, and a second place for the reader to lose a pairing. A file
  in the app's own data folder is the same trust domain as the rest of its
  settings, and deleting it is the same as un-pairing.
- **Pinning the extension ID from the manifest.** Firefox puts a per-install
  UUID in `moz-extension://`, not the add-on ID, so the ID in `manifest.json`
  cannot be compared with the `Origin` header. Pinning what the browser actually
  presents, on first use, gets the same guarantee.
- **A short numeric code, displayed for typing.** Friendlier to read out, weaker
  against a local guesser unless it is rate-limited and rotated; the copy/paste
  path is the one the reader actually uses on the machine that has both windows.
