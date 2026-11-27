import { Progress } from "@/components/ui/progress";
import { Tooltip } from "@/components/ui/tooltip";
import { formatSize } from "@/lib/downloads";
import type { TorrentJob } from "@/lib/ipc-torrent";

interface TorrentFilesProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
}

/** The head of a table of the window: small, quiet and sticky. */
const HEAD =
  "sticky top-0 z-10 bg-chrome-row-alt py-1.5 text-chrome-micro font-normal text-chrome-muted";

/**
 * The files of a job, each one with how much of it is on disk.
 *
 * A torrent can hold hundreds of files, so the header stays while the list
 * scrolls and a file the reader left out says so in words instead of looking
 * like one that has not started.
 */
export function TorrentFiles({ job }: TorrentFilesProps) {
  if (job.files.length === 0) {
    return (
      <p className="py-8 text-center text-xs text-chrome-muted">
        The file list of this torrent is not known yet. A magnet asks its peers
        for it; the list appears here when one answers.
      </p>
    );
  }

  return (
    <table className="w-full table-fixed border-collapse text-xs">
      <thead>
        <tr className="text-left">
          <th scope="col" className={`${HEAD} pl-1`}>
            File
          </th>
          <th scope="col" className={`${HEAD} w-24 pr-3`}>
            Progress
          </th>
          <th scope="col" className={`${HEAD} w-20 pr-2 text-right`}>
            Size
          </th>
        </tr>
      </thead>
      <tbody>
        {job.files.map((file, index) => {
          const kept = file.selected || job.selected.length === 0;
          return (
            <tr
              key={`${index}-${file.path}`}
              className="odd:hover:bg-chrome-popup-hover even:bg-chrome-row-alt even:hover:bg-chrome-popup-hover"
            >
              <td className="py-1.5 pr-3 pl-1">
                <span className="flex min-w-0 items-center gap-2">
                  <Tooltip content={file.path} className="min-w-0 flex-1">
                    <span className="block truncate text-chrome-menu-text">
                      {file.path}
                    </span>
                  </Tooltip>
                  {!kept ? (
                    <span className="shrink-0 text-chrome-micro text-chrome-muted">
                      left out
                    </span>
                  ) : file.done ? (
                    <span className="shrink-0 text-chrome-micro text-chrome-muted">
                      done
                    </span>
                  ) : null}
                </span>
              </td>
              <td className="py-1.5 pr-3">
                <span className="flex items-center gap-2">
                  <Progress
                    value={file.progress * 100}
                    aria-label={`Progress of ${file.path}`}
                    className="h-1 flex-1"
                  />
                  <span className="w-8 shrink-0 text-right text-chrome-micro tabular-nums text-chrome-muted">
                    {Math.round(file.progress * 100)}%
                  </span>
                </span>
              </td>
              <td className="py-1.5 pr-2 text-right text-chrome-micro tabular-nums text-chrome-muted">
                {formatSize(file.length)}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
