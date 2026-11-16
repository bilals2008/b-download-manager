import type { TorrentJob, TorrentStatus } from "@/lib/ipc-torrent";

/** How a job looks in the list: at work, held, done, or broken. */
export type TorrentTone = "active" | "paused" | "done" | "failed";

/** The words a job shows for what it is doing. */
export function statusSentence(status: TorrentStatus): string {
  switch (status.state) {
    case "idle":
      return "Not started yet.";
    case "looking":
      return "Looking for peers.";
    case "noPeers":
      return "No peers yet.";
    case "downloading":
      return "Downloading.";
    case "paused":
      return "Paused.";
    case "completed":
      return "Finished.";
    case "seeding":
      return "Seeding — uploading to the swarm.";
    case "failed":
      return status.message || "This torrent could not be downloaded.";
  }
}

/** The tone a job is shown in. */
export function statusTone(status: TorrentStatus): TorrentTone {
  switch (status.state) {
    case "paused":
      return "paused";
    case "completed":
    case "seeding":
      return "done";
    case "failed":
      return "failed";
    default:
      return "active";
  }
}

/** How much of the job is on disk, from 0 to 1. */
export function fraction(job: TorrentJob): number {
  if (job.totalLength <= 0) {
    return 0;
  }
  return Math.min(1, Math.max(0, job.downloaded / job.totalLength));
}

/** Whether the job has everything it was asked for. */
export function isDone(job: TorrentJob): boolean {
  return job.status.state === "completed" || job.status.state === "seeding";
}

/** How many peers the swarm has, and how many of them have everything. */
export function swarmCounts(job: TorrentJob): string {
  const peers = job.peers.length;
  const parts = [`${peers} ${peers === 1 ? "peer" : "peers"}`];
  if (job.seeders > 0) {
    parts.push(`${job.seeders} ${job.seeders === 1 ? "seeder" : "seeders"}`);
  }
  return parts.join(" · ");
}
