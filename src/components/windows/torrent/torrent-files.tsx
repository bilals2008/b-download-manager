import { Progress } from "@/components/ui/progress";
import { Tooltip } from "@/components/ui/tooltip";
import { formatSize } from "@/lib/downloads";
import type { TorrentJob } from "@/lib/ipc-torrent";

interface TorrentFilesProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
}

/**
 * The files of a job, each one with how much of it is on disk.
 *
 * A torrent can hold hundreds of files, so the list scrolls and the files
 * the reader did not keep are said to be left out rather than drawn empty.
 */
export function TorrentFiles({ job }: TorrentFilesProps) {
  if (job.files.length === 0) {
    return (
      <p className="py-6 text-center text-xs text-chrome-muted">
        The file list of this torrent is not known yet. A magnet asks its peers
        for it.
      </p>
    );
  }

  return (
    <table className="w-full table-fixed border-collapse text-xs">
      <thead>
        <tr className="text-left text-chrome-micro text-chrome-muted">
          <th scope="col" className="pb-1 font-normal">File</th>
          <th scope="col" className="w-20 pb-1 font-normal">Progress</th>
          <th scope="col" className="w-20 pb-1 text-right font-normal">Size</th>
        </tr>
      </thead>
      <tbody>
        {job.files.map((file, index) => {
          const kept = file.selected || job.selected.length === 0;
          return (
            <tr
              key={`${index}-${file.path}`}
              className="border-b border-chrome-line/60 last:border-b-0"
            >
              <td className="py-1.5 pr-3">
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
                  ) : null}
                </span>
              </td>
              <td className="w-20 py-1.5 pr-3">
                <Progress
                  value={file.progress * 100}
                  aria-label={`Progress of ${file.path}`}
                  className="h-1 w-20"
                />
              </td>
              <td className="w-20 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
                {formatSize(file.length)}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
