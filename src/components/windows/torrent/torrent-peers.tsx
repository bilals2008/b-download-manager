import { Tooltip } from "@/components/ui/tooltip";
import { formatSpeed } from "@/lib/downloads";
import type { TorrentJob } from "@/lib/ipc-torrent";
import { cn } from "@/lib/utils";

interface TorrentPeersProps {
  /** The job the reader is looking at. */
  job: TorrentJob;
}

/**
 * The peers of a job: the computers that share it.
 *
 * A peer that is connected carries the mark of the accent and says how much
 * of the torrent it has and what it has sent and taken; one that is only
 * known is waiting for its turn.
 */
export function TorrentPeers({ job }: TorrentPeersProps) {
  if (job.peers.length === 0) {
    return (
      <p className="py-6 text-center text-xs text-chrome-muted">
        No peers yet. The trackers and the peer table are asked again and
        again; a torrent nobody shares stays empty.
      </p>
    );
  }

  return (
    <table className="w-full table-fixed border-collapse text-xs">
      <thead>
        <tr className="text-left text-chrome-micro text-chrome-muted">
          <th scope="col" className="pb-1 font-normal">Peer</th>
          <th scope="col" className="w-28 pb-1 font-normal">Client</th>
          <th scope="col" className="w-12 pb-1 text-right font-normal">Have</th>
          <th scope="col" className="w-20 pb-1 text-right font-normal">Down</th>
          <th scope="col" className="w-20 pb-1 text-right font-normal">Up</th>
        </tr>
      </thead>
      <tbody>
        {job.peers.map((peer) => (
          <tr key={peer.address} className="border-b border-chrome-line/60 last:border-b-0">
            <td className="py-1.5 pr-3">
              <span className="flex min-w-0 items-center gap-2">
                <span
                  aria-hidden="true"
                  className={cn(
                    "size-1.5 shrink-0 rounded-full",
                    peer.connected ? "bg-primary" : "bg-chrome-muted",
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
            <td className="w-28 py-1.5 pr-3">
              {peer.client ? (
                <Tooltip content={peer.client} className="w-full">
                  <span className="block truncate text-chrome-micro text-chrome-muted">
                    {peer.client}
                  </span>
                </Tooltip>
              ) : null}
            </td>
            <td className="w-12 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
              {Math.round(peer.progress * 100)}%
            </td>
            <td className="w-20 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
              {formatSpeed(peer.downSpeed)}
            </td>
            <td className="w-20 py-1.5 text-right text-chrome-micro tabular-nums text-chrome-muted">
              {formatSpeed(peer.upSpeed)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}