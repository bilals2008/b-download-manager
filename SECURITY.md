# Security Policy

## Supported versions

Only the latest published release receives security fixes. Development builds
from `main` are unsupported snapshots.

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |
| < 0.1   | No        |

## Reporting a vulnerability

**Do not open a public issue.** Report privately to **wbiz2939@gmail.com** with:

- the version you tested and your operating system
- what the issue lets an attacker do, and what access it needs to start
- reproduction steps or a proof of concept
- any logs, with personal data removed

You will get an acknowledgement within 72 hours and an assessment within seven
days. Once a fix is ready, the advisory is published after a release that
contains it, and reporters are credited unless they ask otherwise. Please give
the maintainer a reasonable window to ship a fix before disclosing publicly.

## In scope

- **Update path.** The app verifies `latest.json` and the downloaded installer
  against the minisign public key baked into `src-tauri/tauri.conf.json`. A way
  to make an install accept an unsigned or foreign-signed update is the highest
  severity issue in this project. See [docs/RELEASES.md](docs/RELEASES.md).
- **Download engine.** Path traversal through filenames or server-supplied
  `Content-Disposition`, writes outside the chosen folder, or a crafted response
  that corrupts other files on disk.
- **Browser extension bridge.** A web page reaching privileged app commands
  through the extension, or a message that runs with more trust than the origin
  that sent it.
- **Tauri surface.** An IPC command reachable from frontend content with the
  wrong capability, or a CSP/capability configuration that widens what a loaded
  resource can do.
- **Credential and header handling.** Leaking cookies, proxy credentials or
  authentication headers to a redirect target or into logs.

## Out of scope

- Issues in dependencies that are already public and have no exploit path in
  this app (report them upstream as well, but they are not an app advisory).
- Missing hardening with no demonstrated impact, self-XSS, or attacks that need
  an already-compromised machine or a modified local build.
- Denial of service through absurdly large downloads that the user started
  themselves.

## Safe harbour

Good-faith research on your own machine and against your own servers is
welcome. Do not test against third-party infrastructure, do not access other
people's data, and do not leave a proof of concept running in the wild.
