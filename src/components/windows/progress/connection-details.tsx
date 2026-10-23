import { formatSize } from "@/lib/downloads";
import {
  connectionLabel,
  connectionSummary,
  segmentSize,
} from "@/lib/progress";
import { cn } from "@/lib/utils";
import type {
  DownloadSegment,
  DownloadStatus,
  SegmentStatus,
} from "@/types";

/** A dot per state: alive, finished, stopped, broken, or simply quiet. */
const DOT: Record<SegmentStatus, string> = {
  pending: "bg-chrome-muted",
  connecting: "bg-chrome-muted",
  receiving: "bg-progress",
  retrying: "bg-chrome-muted",
  paused: "bg-chrome-muted",
  stopped: "bg-chrome-muted",
  done: "bg-success",
  failed: "bg-destructive",
};

const TONE: Record<SegmentStatus, string> = {
  pending: "text-chrome-muted",
  connecting: "text-chrome-muted",
  receiving: "text-chrome-menu-text",
  retrying: "text-chrome-muted",
  paused: "text-chrome-muted",
  stopped: "text-chrome-muted",
  // The success token is a fill, not a text colour: the dot carries the
  // state and the word stays the readable menu text in both appearances.
  done: "text-chrome-menu-text",
  failed: "text-chrome-error",
};

/**
 * One row per connection: what it has, and what it is doing.
 *
 * This is the part of the window the Hide details control folds away, so it
 * scrolls inside the height the window gives it rather than growing the
 * window for every extra connection.
 */
export function ConnectionDetails({
  segments,
  status,
}: {
  segments: DownloadSegment[];
  status: DownloadStatus;
}) {
  // A download restored from the index has no ranges: they were joined and
  // removed when it finished. That is not a plan waiting to be made.
  const joined = status === "completed" && segments.length === 0;

  return (
    <div
      id="progress-connections"
      className="flex min-h-0 flex-1 flex-col border-t border-chrome-line pt-2.5"
    >
      <div className="flex items-baseline justify-between gap-3 pb-1">
        <h3 className="text-chrome-micro font-medium text-chrome-muted">
          Connections
        </h3>
        <span className="text-chrome-micro tabular-nums text-chrome-muted">
          {joined ? "No connections open" : connectionSummary(segments)}
        </span>
      </div>

      <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto">
        <table className="w-full table-fixed border-separate border-spacing-0 text-chrome-micro">
          <caption className="sr-only">
            Connections of this download, with the bytes each one has received
            and what it is doing
          </caption>
          <thead>
            {/* The header stays put while the connections scroll under it, so
                it carries the surface itself and closes on a hairline. */}
            <tr className="text-chrome-muted">
              <th
                scope="col"
                className="sticky top-0 w-10 border-b border-chrome-line bg-chrome-popup px-2 py-1.5 text-left font-semibold tracking-wider uppercase"
              >
                #
              </th>
              <th
                scope="col"
                className="sticky top-0 w-28 border-b border-chrome-line bg-chrome-popup px-2 py-1.5 text-right font-semibold tracking-wider uppercase"
              >
                Downloaded
              </th>
              <th
                scope="col"
                className="sticky top-0 border-b border-chrome-line bg-chrome-popup px-2 py-1.5 text-left font-semibold tracking-wider uppercase"
              >
                Status
              </th>
            </tr>
          </thead>
          <tbody>
            {segments.length === 0 ? (
              <tr>
                <td
                  colSpan={3}
                  className="px-2 py-4 text-center text-chrome-muted"
                >
                  {joined
                    ? "The file is complete; its parts were joined and cleaned up."
                    : "The engine has not opened a connection yet."}
                </td>
              </tr>
            ) : (
              segments.map((segment, index) => (
                <tr key={segment.id} className="even:bg-chrome-row-alt">
                  <td className="px-2 py-1.5 tabular-nums text-chrome-muted">
                    {index + 1}
                  </td>
                  <td
                    title={
                      segment.size === null
                        ? `Connection ${index + 1}: ${formatSize(segment.downloaded)} received; the server did not tell the file size`
                        : `Connection ${index + 1}: ${formatSize(segment.downloaded)} of ${segmentSize(segment)}`
                    }
                    className="px-2 py-1.5 text-right tabular-nums text-chrome-menu-text"
                  >
                    {formatSize(segment.downloaded)}
                  </td>
                  <td className="px-2 py-1.5">
                    <span
                      className={cn(
                        "inline-flex items-center gap-1.5",
                        TONE[segment.status],
                      )}
                    >
                      <span
                        aria-hidden="true"
                        className={cn(
                          "size-1.5 shrink-0 rounded-full",
                          DOT[segment.status],
                        )}
                      />
                      {connectionLabel(segment.status)}
                    </span>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
