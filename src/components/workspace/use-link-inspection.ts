//! What a pasted address turns out to be: the server's answer about the file,
//! and whether the page is a gallery of images.
//!
//! Both checks are advice, never a gate. A server that will not answer the
//! probe can still be downloaded from, and a page that cannot be read as a
//! gallery is still an ordinary download. Keeping them here means the form
//! only has to ask one question: what does this address hold?

import { useEffect, useState } from "react";

import { describeError, isDesktop } from "@/lib/errors";
import {
  galleryInspect,
  type GalleryInspect,
} from "@/lib/ipc-gallery";
import { downloadUrl, probeWarning } from "@/lib/downloads";
import { probeLink } from "@/lib/ipc";

/** What the server said about the address, in the order the dialog asks. */
export type Probe =
  | { kind: "idle" }
  | { kind: "checking" }
  | {
      kind: "ready";
      name: string | null;
      total: number | null;
      resumable: boolean;
    }
  | { kind: "failed"; reason: string };

interface LinkInspectionOptions {
  open: boolean;
  /** The address as it was pasted, before any redirect. */
  address: string;
  /** The address the bytes would actually come from (a Drive export, say). */
  probeAddress: string;
  referer: string;
  /** What the user is editing right now; a stale answer is dropped. */
  editing: string;
}

export function useLinkInspection({
  open,
  address,
  probeAddress,
  referer,
  editing,
}: LinkInspectionOptions) {
  const [probe, setProbe] = useState<Probe>({ kind: "idle" });
  const [gallery, setGallery] = useState<GalleryInspect | null>(null);

  // Ask the server what the address holds once it has settled: the name it
  // gives the file, the size, and whether a stop could resume.
  useEffect(() => {
    setProbe({ kind: "idle" });
    if (!open || !isDesktop() || !downloadUrl(editing.trim())) {
      return;
    }
    let active = true;
    let deadline: ReturnType<typeof setTimeout> | undefined;
    setProbe({ kind: "checking" });
    const timer = setTimeout(() => {
      // A server that accepts the connection and then says nothing would keep
      // the check spinning forever. The download can still start, so the
      // dialog says so and stops waiting.
      const tooLong = new Promise<"too long">((resolve) => {
        deadline = setTimeout(() => resolve("too long"), 8_000);
      });
      Promise.race([probeLink(probeAddress, referer), tooLong])
        .then((answer) => {
          if (!active) {
            return;
          }
          if (answer === "too long") {
            setProbe({
              kind: "failed",
              reason:
                "The site did not answer in time. You can still start the download.",
            });
            return;
          }
          setProbe({
            kind: "ready",
            name: answer.name,
            total: answer.total,
            resumable: answer.resumable,
          });
        })
        .catch((failure: unknown) => {
          if (active) {
            setProbe({
              kind: "failed",
              reason: probeWarning(describeError(failure)),
            });
          }
        })
        .finally(() => {
          if (deadline) {
            clearTimeout(deadline);
          }
        });
    }, 450);
    return () => {
      active = false;
      clearTimeout(timer);
      if (deadline) {
        clearTimeout(deadline);
      }
    };
  }, [open, probeAddress, referer, editing]);

  // A page that reads as a gallery is offered as one: the same link, the
  // count of images, one click. The fetch is the dialog's own, advice only.
  useEffect(() => {
    setGallery(null);
    if (!open || !isDesktop() || !downloadUrl(address.trim())) {
      return;
    }
    let active = true;
    const timer = setTimeout(() => {
      galleryInspect(address.trim(), referer || undefined)
        .then((answer) => {
          if (active) {
            setGallery(answer);
          }
        })
        .catch(() => {
          // A page that does not answer still gets an ordinary download.
        });
    }, 450);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [open, address, referer]);

  return { probe, gallery };
}
