# Contributing

Thanks for taking the time to improve B Download Manager. This project is a
Tauri 2 desktop app: a React 19 frontend in `src/`, a Rust engine in
`src-tauri/` and reusable crates in `crates/`.

## Before you start

- Open an issue for anything larger than a bug fix, so the design is agreed
  before code is written.
- Report security problems privately, never in a public issue. See
  [SECURITY.md](SECURITY.md).
- By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Setup

Requirements are listed in [README.md](README.md). Then:

```bash
pnpm install
pnpm tauri dev
```

`docs/DEVELOPMENT.md` explains the architecture, the download engine and the
conventions for the UI layer.

## Checks

Everything CI runs, locally. A change is ready when all of these pass:

```bash
pnpm build                              # tsc --noEmit + vite build
pnpm check:palette                      # and the other check:* scripts
cargo fmt --all --manifest-path src-tauri/Cargo.toml --check
cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Feature-specific `check:*` scripts exist for the parts they cover (progress,
add dialog, categories, settings, updater, extension bridge). Run the ones
touching what you changed, and the full set in CI.

## Style

- **Frontend:** TypeScript, React function components, Tailwind CSS v4. Colors,
  spacing and radii come from the semantic tokens in `src/index.css`; never
  hardcode a design value in a component. Avoid `useEffect` unless the effect is
  genuinely synchronising with something outside React.
- **Rust:** run `cargo fmt` and keep clippy clean at `-D warnings`.
- **Reusable code:** extract hooks, utilities and components instead of copying
  them, and keep files organised by feature and responsibility.
- **Icons and labels:** use the shared `Tooltip` for icon-only controls rather
  than a native `title` attribute.

## Commits and pull requests

- Use conventional commit messages: `feat:`, `fix:`, `perf:`, `docs:`,
  `refactor:`, `chore:`, `test:`. The release notes are generated from them.
- Keep one logical change per commit and one purpose per pull request.
- Write commit subjects in the imperative mood, in English, with no trailing
  period.
- State what changed and why in the pull request, and note anything you could
  not verify.
- Do not commit build output, installers or `.env` files. `.gitignore` covers
  them; if you had to bypass it, explain why in the pull request.

## Releases

Releases are cut by maintainers only:

```bash
pnpm release:patch    # or :minor / :major
```

That command bumps every version file, writes the CHANGELOG section, commits,
tags and pushes; the tag triggers the signed Windows build and the GitHub
release. See [docs/RELEASES.md](docs/RELEASES.md) before touching anything in
the release or updater path.

## License

Contributions are accepted under the [MIT License](LICENSE).
