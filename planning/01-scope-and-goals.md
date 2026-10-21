# Scope and goals

## The goal

A download manager that behaves like IDM in the ways users actually notice:

- a large file is pulled by several connections at once, not one;
- pausing, resuming and restarting a download keep the bytes already fetched;
- a download survives a crash, a reboot and a network drop;
- a server that misbehaves is detected instead of producing a corrupt file.

It does not have to look like IDM, and it does not have to implement every
protocol IDM does. The bar is: **on a well-behaved HTTP server, it downloads a
large file at least as fast as a single-connection client, and it can always
resume.**

## In scope

| Capability | Why |
|---|---|
| HTTP/1.1 with byte ranges | the core mechanism |
| HTTPS | almost everything is HTTPS now |
| Multi-connection segmentation | the whole point |
| Per-connection part files | what makes resume cheap and parallel writes safe |
| Manifest-based resume | crash safety |
| Pause / resume / cancel | user expectation |
| Dynamic connection growth | a link that is faster than one connection should be used |
| Server integrity verification | a lying server must not produce a corrupt file |
| Retry with backoff | transient failures are normal |
| Disk space checks | a failed download at 99% is a bad experience |
| Speed limiting and a queue | IDM has both; they are the difference between a toy and a tool |

## Deliberately out of scope

| Not doing | Why |
|---|---|
| Copying any IDM code, strings or assets | licence and ethics; also the point of the exercise is the reimplementation |
| Browser integration (extension, native messaging) | large surface, and it is about *capture*, not *downloading* |
| A grabber / site scraper | a different product |
| RTMP, MMS, FTP, SOCKS, proxy chains | IDM supports them; they are each a project of their own. They are listed as post-MVP in the milestone plan, not dropped. |
| DRM circumvention | out of bounds |

## Non-goals that are easy to get wrong

- **Not** "match IDM's UI". Match IDM's *mechanics*; the visual language is
  ours (see [05-ui-design-direction.md](./05-ui-design-direction.md)).
- **Not** "open a socket per file". Connection count is per download, capped
  globally, and the two limits are separate.
- **Not** "assume the server honours Range". IDM does not assume it either,
  and neither do we.

## Success criteria

The product is done when all of these hold and are demonstrated, not asserted:

1. A 1 GiB file from a range-capable server downloads at a rate that scales
   with the connection count, measured on the same machine against a
   single-connection baseline.
2. Killing the process mid-download and restarting it resumes from the byte
   where it stopped, and the final file is byte-identical to the source.
3. A server that returns `200` for a range request produces a single
   connection and a correct file — not a corrupt one.
4. A server that returns the wrong bytes for a range is detected and the
   download is retried or downgraded, and the user is told.
5. The connection limit is honoured globally, and the UI shows the real number
   of live connections.
6. No colour, contrast or accessibility claim in the docs is unmeasured.
