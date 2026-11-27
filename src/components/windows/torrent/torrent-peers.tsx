import { Tooltip } from "@/components/ui/tooltip";
import { formatSpeed } from "@/lib/downloads";
import type { TorrentJob } from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

interface TorrentPeersProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
}

/** The head of a table of the window: small, quiet and sticky. */
const HEAD =
  "sticky top-0 z-10 bg-chrome-row-alt py-1.5 text-chrome-micro font-normal text-chrome-muted";

/**
 * The peers of a job: the computers that share it.
 *
 * A peer that is connected carries the accent dot; one that is only known is
 * grey and waiting for its turn. A peer that sends nothing says so with a
 * dash instead of a zero, because a zero speed is a number and no traffic is
 * the absence of one.
 */
export function TorrentPeers({ job }: TorrentPeersProps) {
  if (job.peers.length === 0) {
    return (
      <p className="py-8 text-center text-xs text-chrome-muted">
        No peers yet. The trackers and the peer table are asked again and
        again; a torrent nobody shares stays empty.
      </p>
    );
  }

  const connected = job.peers.filter((peer) => peer.connected).length;

  return (
    <div className="flex flex-col gap-2 pb-2">
      <p className="text-chrome-micro tabular-nums text-chrome-muted">
        {connected} connected · {job.peers.length} known ·{" "}
        {job.seeders} {job.seeders === 1 ? "seeder" : "seeders"}
      </p>
      <table className="w-full table-fixed border-collapse text-xs">
        <thead>
          <tr className="text-left">
            <th scope="col" className={`${HEAD} pl-1`}>
              Peer
            </th>
            <th scope="col" className={`${HEAD} w-32`}>
              Client
            </th>
            <th scope="col" className={`${HEAD} w-14 text-right`}>
              Have
            </th>
            <th scope="col" className={`${HEAD} w-20 text-right`}>
              Down
            </th>
            <th scope="col" className={`${HEAD} w-20 pr-2 text-right`}>
              Up
            </th>
          </tr>
        </thead>
        <tbody>
          {job.peers.map((peer) => (
            <tr
              key={peer.address}
              className="odd:hover:bg-chrome-popup-hover even:bg-chrome-row-alt even:hover:bg-chrome-popup-hover"
            >
              <td className="py-1.5 pr-3 pl-1">
                <span className="flex min-w-0 items-center gap-2">
                  <span
                    aria-hidden="true"
                    className={cn(
                      "size-1.5 shrink-0 rounded-full",
                      peer.connected ? "bg-chrome-focus" : "bg-chrome-line",
                    )}
                  />
                  <span className="sr-only">
                    {peer.connected ? "Connected:" : "Known:"}
                  </span>
                  <Tooltip content={peer.address} className="min-w-0 flex-1">
                    <span className="block truncate tabular-nums text-chrome-menu-text">
                      {peer.address}
                    </span>
                  </Tooltip>
                </span>
              </td>
              <td className="w-32 py-1.5 pr-3">
                {peer.client ? (
                  <Tooltip content={peer.client} className="w-full">
                    <span className="block truncate text-chrome-micro text-chrome-muted">
                      {peer.client}
                    </span>
                  </Tooltip>
                ) : (
                  <span className="text-chrome-micro text-chrome-muted">—</span>
                )}
              </td>
              <td className="w-14 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
                {Math.round(peer.progress * 100)}%
              </td>
              <td className="w-20 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
                {peer.downSpeed > 0 ? formatSpeed(peer.downSpeed) : "—"}
              </td>
              <td className="w-20 py-1.5 pr-2 text-right text-chrome-micro tabular-nums text-chrome-muted">
                {peer.upSpeed > 0 ? formatSpeed(peer.upSpeed) : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
