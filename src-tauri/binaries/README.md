# Sidecar binaries

The files in this folder are build products; they are not kept in git.

| File | What it is | How it is made |
|---|---|---|
| `ffmpeg-*` | The FFmpeg this app fetches and verifies, when the user chooses to download from a site that needs one | fetched by the app, see the video settings page |
| `idm-mcp-<target-triple>.exe` | The MCP server an AI assistant starts to list, add, pause, resume and remove downloads. Only used while "let AI assistants drive the app" is on in Settings > Advanced | `cargo build --release -p idm-mcp`, then copy `target/release/idm-mcp.exe` to `src-tauri/binaries/idm-mcp-<target-triple>.exe` (e.g. `idm-mcp-x86_64-pc-windows-msvc.exe`) before `pnpm tauri build` |

The `externalBin` entry in `tauri.conf.json` picks the file up in the NSIS
installer, so the assistant finds the program beside the app.
