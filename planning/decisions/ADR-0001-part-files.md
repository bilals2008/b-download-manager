# ADR 0001 — one part file per range, written at absolute offsets

- **Status:** accepted
- **Date:** 2026-10-21
- **Relates to:** [../04-engine-spec.md](../04-engine-spec.md) §4

## Context

A multi-connection download has to answer three questions at once: which
bytes of the target file each connection owns, where those bytes are written
while they are in flight, and how to resume if the process dies.

The reference product answers them with a chunk list.
`ChunksList::CreateNewItem()` allocates
a `ChunksListItem`, names a file from a directory plus a counter
(`L"%s%ld"`), and opens it with `GENERIC_READ | GENERIC_WRITE` and
`FILE_SHARE_READ`. The chunk writer `FUN_0055db20` then seeks to an absolute
offset with `SetFilePointer(handle, offset, NULL, FILE_BEGIN)` before every
`WriteFile`.

That is the design under discussion, and it is worth being explicit about
because the obvious alternatives look better and behave worse.

## Options considered

**A. One part file per range, absolute offsets.** *(chosen)*

**B. One shared pre-allocated file, one write cursor per connection.** The
file is created at full size and each connection writes with a positional
write. Simple, and one file on disk.

Rejected: a shared file needs either a lock per write or one file handle per
connection, and it makes the resume state ambiguous — "which bytes of the
shared file belong to which range" is not derivable from the file alone.
It also means a partially written file looks like a complete one.

**C. One buffer in memory, flushed to the destination at the end.** The
simplest possible thing.

Rejected: it caps the file size at the free memory, and it loses everything
on a crash. The studied binary is explicit that this is not what it does.

**D. Write into the destination file directly, at the right offset, from the
start.** No parts, no assembly step.

Rejected for the general case: the destination path is often not writable
until the download is complete (a file being downloaded is usually open in
another program, on a network share, or in a folder that is being synced),
and a partial file at the destination is indistinguishable from a finished
one. The reference product avoids this too: it assembles into a temporary
file and renames it into place, which is atomic from the user's point of view.

## Decision

**Option A.** Each range owns a part file. Bytes are written at their
absolute offset inside that part file. The parts are joined in range order
into a temporary file, which is then renamed onto the destination.

The manifest records, per range, the byte range and how many bytes of it have
landed. On resume, each part file is truncated to the recorded length, so a
crash that left extra bytes cannot corrupt the result.

## Consequences

**Good.**

- A connection can be stopped at any byte and resumed at the same byte.
- A second connection can take over the tail of a range, because both write
  into the same coordinate system. This is what makes the reference product's
  "add an alternative connection" possible, and it is why the split point is
  the parent's current position.
- The resume state is derivable from the manifest plus the part files. There
  is no hidden state in the engine.
- The assembly step is a plain sequential copy, which is fast and easy to
  verify.

**Bad.**

- The disk cost peaks at roughly twice the file size: the parts plus the
  output. The parts are deleted immediately after the rename, but the peak
  is real and has to be checked before starting a large download.
- One file handle per live connection, plus one for the output during
  assembly. Bounded by the global connection limit.
- The manifest is a second thing that can be corrupted. It is written
  atomically — write to a temporary name, then rename.

**Neutral.**

- Part file names are an implementation detail. The current scheme is
  `<destination stem>.part<N>`, which is close to the reference product's own
  `<folder><counter>` naming, but the exact name is ours to choose.
