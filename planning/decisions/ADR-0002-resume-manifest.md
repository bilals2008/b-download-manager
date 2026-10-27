# ADR 0002 — the resume manifest is a JSON file next to the parts

- **Status:** accepted
- **Date:** 2026-10-21
- **Relates to:** [../04-engine-spec.md](../04-engine-spec.md) §7

## Context

A download that is interrupted has to be resumable. That means the state of
the download — what was being fetched, which byte ranges existed, how many
bytes of each landed, where the parts are on disk — has to outlive the
process.

The reference product keeps this in the registry:
`HKCU\Software\DownloadManager\<id>\ChList`, one 16-byte value per chunk
named `"<n>a"`, plus `LocalFileName` for the target path.

## Options considered

**A. A JSON file next to the part files.** *(chosen)*

**B. The Windows registry, like the reference product.** A key per download
under `HKCU\Software\DownloadManager\<id>`, one value per chunk.

**C. A SQLite database in the app data directory.**

**D. Nothing — infer the state from the part files on disk.**

## Decision

**Option A.** A JSON file, `<destination stem>.idm.json`, written atomically
next to the part files.

```json
{
  "url": "https://example.com/file.zip",
  "total": 1234567890,
  "segments": [
    { "id": 0, "start": 0, "end": 524288000, "received": 524288000, "part": "file.zip.part0" }
  ]
}
```

## Rationale

**Why not the registry (B).** The publisher's choice makes sense for the
reference product: it is a Windows application that must interoperate with
its own browser extensions, its tray agent and its scheduler, all of which
are separate processes that read the same keys. We have none of that. The
registry also means a registry-specific implementation on a platform we
might want to run on elsewhere, and a corrupt registry key is a much worse
failure mode than a corrupt file we wrote ourselves.

The one thing the registry gives that a file does not is a *stable id* that
is independent of the file system. We get that from the destination path
instead, which is what the manifest is keyed on.

**Why not SQLite (C).** Not yet. The queries we need are "list everything"
and "find the one for this destination", and a JSON file answers both in
microseconds at any realistic download count. A database earns its
complexity when there are hundreds of downloads, a search index and a
history that must be queried across fields. If the milestone plan ever adds
a download history, this decision is revisited.

**Why not nothing (D).** The part files alone do not say what range each one
covers, nor how many bytes of it are valid. A crash can leave a part file
longer than the bytes that were actually received and verified. Without a
manifest, the only safe assumption is that every part file is empty, which
throws away the work.

## Consequences

**Good.**

- Portable and inspectable. A user can read the file. So can we, when
  something goes wrong.
- Versionable and diffable, which makes resume-format changes reviewable.
- Travels with the download. Copy the parts and the manifest to another
  machine and the download continues.
- No platform-specific API. Works the same on every OS.

**Bad.**

- The manifest and the part files can disagree if the user deletes one and
  not the other. Handled by treating a missing part file as "that range is
  gone" and re-planning.
- It sits in the download directory, which may be read-only, on a network
  share, or cleaned up by the user. Fallback: the app data directory, keyed
  by a hash of the destination path. That is a fallback, not the default.
- Two files to keep consistent. Mitigated by writing the manifest
  atomically and treating it as authoritative only when it parses and its
  URL and total match the server's answer.

**Neutral.**

- The manifest is *ours*, not the reference product's. We are not reading
  its `ChList`, and we do not intend to. If that ever changes, it needs its
  own decision record and a careful look at the licence question in
  [../08-risks-and-open-questions.md](../08-risks-and-open-questions.md) R1.
