import { useEffect, useMemo, useRef, useState } from "react";

import { SpinnerGlyph } from "@/components/icons/ui-icons";
import { formatSize } from "@/lib/downloads";
import {
  extractFormats,
  ffmpegAvailable,
  type VideoFormat,
} from "@/lib/ipc";

/** A format the reader picked, with everything a download needs. */
export interface VideoQualitySelection {
  /** A stable key for the row. */
  id: string;
  /** Whether the file is complete, or must be muxed with `audioUrl`. */
  kind: "muxed" | "video-only" | "audio-only";
  videoUrl: string;
  /** The sound to mux with, or null for a complete or audio-only file. */
  audioUrl: string | null;
  container: string;
  fileName: string;
}

interface VideoQualityPickerProps {
  url: string;
  /** The file name the dialog already worked out, without a folder. */
  baseName: string;
  onSelect: (selection: VideoQualitySelection | null) => void;
}

const ROW =
  "group flex h-9 w-full min-w-0 items-center gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 text-left transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none data-[selected=true]:border-chrome-focus data-[selected=true]:bg-chrome-inset disabled:cursor-default disabled:opacity-60";

type Status = "idle" | "loading" | "ready" | "failed";

/** The stem of a file name, with any media extension taken off. */
function stemOf(name: string): string {
  const cleaned = name.trim().replace(/\s+/g, " ");
  const dot = cleaned.lastIndexOf(".");
  if (dot > 0 && cleaned.length - dot <= 5) {
    return cleaned.slice(0, dot);
  }
  return cleaned || "video";
}

function qualityLabel(format: VideoFormat): string {
  const bits = [];
  if (format.resolution) bits.push(format.resolution);
  if (format.fps) bits.push(`${Math.round(format.fps)} fps`);
  bits.push(format.ext.toUpperCase());
  if (format.filesize) bits.push(formatSize(format.filesize));
  return bits.join(" · ");
}

/**
 * "Choose quality" for a video address pasted into the Add URL dialog.
 *
 * The list is read on demand rather than on every keystroke: the extractor is
 * a heavier call than the link probe, so it waits for the reader to ask. When
 * the page offers separate picture and sound streams, the two are muxed on
 * download; a stream that already carries both is downloaded as it is.
 */
export function VideoQualityPicker({
  url,
  baseName,
  onSelect,
}: VideoQualityPickerProps) {
  const [status, setStatus] = useState<Status>("idle");
  const [formats, setFormats] = useState<VideoFormat[]>([]);
  const [ffmpeg, setFfmpeg] = useState(false);
  const [error, setError] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const running = useRef(false);

  // A new address starts over: the formats of the last one are not this one's.
  useEffect(() => {
    setStatus("idle");
    setFormats([]);
    setError("");
    setSelectedId(null);
    onSelect(null);
  }, [url, onSelect]);

  const videos = useMemo(
    () =>
      formats
        .filter((format) => format.kind === "video-only")
        .sort(
          (left, right) =>
            (right.height ?? 0) - (left.height ?? 0) ||
            (right.filesize ?? 0) - (left.filesize ?? 0),
        ),
    [formats],
  );
  const muxed = useMemo(
    () =>
      formats
        .filter((format) => format.kind === "muxed")
        .sort(
          (left, right) =>
            (right.height ?? 0) - (left.height ?? 0) ||
            (right.filesize ?? 0) - (left.filesize ?? 0),
        ),
    [formats],
  );
  const audios = useMemo(
    () =>
      formats
        .filter((format) => format.kind === "audio-only")
        .sort((left, right) => (right.filesize ?? 0) - (left.filesize ?? 0)),
    [formats],
  );
  const bestAudio = audios[0] ?? null;

  const options = useMemo(() => {
    const stem = stemOf(baseName);
    const out: { id: string; label: string; detail: string; selection: VideoQualitySelection }[] = [];

    if (ffmpeg && videos.length && bestAudio) {
      const video = videos[0];
      out.push({
        id: "best",
        label: "Best quality",
        detail: `${video.resolution || "video"} + audio`,
        selection: {
          id: "best",
          kind: "video-only",
          videoUrl: video.url ?? "",
          audioUrl: bestAudio.url ?? "",
          container: video.ext,
          fileName: `${stem}.${video.ext}`,
        },
      });
    }

    for (const video of videos) {
      const id = `video-${video.formatId}`;
      out.push({
        id,
        label: video.resolution || "Video",
        detail: qualityLabel(video),
        selection: {
          id,
          kind: "video-only",
          videoUrl: video.url ?? "",
          audioUrl: bestAudio?.url ?? null,
          container: video.ext,
          fileName: `${stem} ${(video.resolution || "video").toLowerCase()}.${video.ext}`,
        },
      });
    }

    for (const format of muxed) {
      const id = `muxed-${format.formatId}`;
      out.push({
        id,
        label: format.resolution || "Video with sound",
        detail: qualityLabel(format),
        selection: {
          id,
          kind: "muxed",
          videoUrl: format.url ?? "",
          audioUrl: null,
          container: format.ext,
          fileName: `${stem} ${(format.resolution || "video").toLowerCase()}.${format.ext}`,
        },
      });
    }

    for (const format of audios) {
      const id = `audio-${format.formatId}`;
      out.push({
        id,
        label: "Audio",
        detail: qualityLabel(format),
        selection: {
          id,
          kind: "audio-only",
          videoUrl: format.url ?? "",
          audioUrl: null,
          container: format.ext,
          fileName: `${stem} audio.${format.ext}`,
        },
      });
    }

    return out;
  }, [baseName, videos, muxed, audios, bestAudio, ffmpeg]);

  async function check() {
    if (running.current) return;
    running.current = true;
    setStatus("loading");
    setError("");
    setFormats([]);
    setSelectedId(null);
    onSelect(null);

    try {
      const [found, hasFfmpeg] = await Promise.all([
        extractFormats(url),
        ffmpegAvailable(),
      ]);
      setFfmpeg(hasFfmpeg);
      const usable = found.filter((format) => format.url);
      if (usable.length === 0) {
        setStatus("failed");
        setError("No downloadable quality was found for this video.");
        return;
      }
      setFormats(usable);
      setStatus("ready");
    } catch (failure) {
      setStatus("failed");
      setError(
        failure instanceof Error
          ? failure.message
          : "The qualities could not be read.",
      );
    } finally {
      running.current = false;
    }
  }

  function choose(id: string) {
    const option = options.find((entry) => entry.id === id);
    if (!option) return;
    setSelectedId(id);
    onSelect(option.selection);
  }

  return (
    <div className="flex flex-col gap-2">
      {status === "idle" || status === "failed" ? (
        <button
          type="button"
          onClick={() => void check()}
          className="inline-flex h-9 items-center gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2.5 text-left text-xs text-chrome-menu-text transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
        >
          <span className="min-w-0 truncate">
            {status === "failed" ? "Try again" : "Choose video quality"}
          </span>
        </button>
      ) : null}

      {status === "failed" && error ? (
        <p role="status" className="text-chrome-micro text-chrome-error">
          {error}
        </p>
      ) : null}

      {status === "loading" ? (
        <p role="status" className="flex items-center gap-1.5 text-chrome-micro text-chrome-muted">
          <SpinnerGlyph className="size-3.5 shrink-0 motion-safe:animate-spin" />
          Reading qualities…
        </p>
      ) : null}

      {status === "ready" ? (
        <ul className="flex max-h-56 flex-col gap-1 overflow-y-auto" role="listbox" aria-label="Video quality">
          {options.map((option) => (
            <li key={option.id}>
              <button
                type="button"
                role="option"
                aria-selected={selectedId === option.id}
                data-selected={selectedId === option.id || undefined}
                onClick={() => choose(option.id)}
                disabled={option.selection.kind === "video-only" && (!option.selection.audioUrl || !ffmpeg)}
                className={ROW}
              >
                <span className="min-w-0 flex-1 truncate text-xs text-chrome-title-text">
                  {option.label}
                </span>
                <span className="shrink-0 truncate text-chrome-micro text-chrome-muted">
                  {option.detail}
                </span>
              </button>
            </li>
          ))}
          {!ffmpeg ? (
            <li className="text-chrome-micro text-chrome-muted">
              Picture-only qualities are hidden because the muxing tools are not
              available in this build.
            </li>
          ) : null}
        </ul>
      ) : null}
    </div>
  );
}
