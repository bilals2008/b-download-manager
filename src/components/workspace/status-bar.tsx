import { useEffect, useState } from "react";

import { formatSize, formatSpeed } from "@/lib/downloads";
import { isDesktop } from "@/lib/errors";
import { onNetworkState, networkState, type NetworkState } from "@/lib/ipc-network";
import { queueSchedule, type ScheduleStatus } from "@/lib/ipc-schedule";
import { useT } from "@/i18n";
import { useDownloads } from "@/state/download-store";

/** One panel of the status bar. Neighbours are separated by space alone. */
const CELL =
  "flex shrink-0 items-center gap-1 px-3 tabular-nums";

/**
 * The status bar along the bottom of the window: how many downloads the
 * current category holds, how many are active or selected, and the total
 * transfer rate. Panels are separated by rules, the way a desktop status bar
 * is, and the size and the active count step aside on a narrow window so the
 * rate always has room.
 *
 * The chosen network adapter is shown too, when there is one: an adapter that
 * is lost is a state the reader must see, not infer from a download that
 * stopped.
 */
export function StatusBar() {
  const { visible, selected, downloads } = useDownloads();
  const t = useT();
  const [network, setNetwork] = useState<NetworkState | null>(null);
  const [schedule, setSchedule] = useState<ScheduleStatus | null>(null);

  // When the queue is scheduled, the bar says when it runs next: "queue runs
  // at 01:00" is the whole feature for someone who forgets what they set.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    const read = () => {
      void queueSchedule()
        .then((status) => {
          if (active) {
            setSchedule(status);
          }
        })
        .catch(() => {
          // An engine that does not answer leaves the bar as it is.
        });
    };
    read();
    const timer = window.setInterval(read, 60_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    let off: (() => void) | undefined;
    // What is in force now, and what changes later.
    void networkState()
      .then((state) => {
        if (active) {
          setNetwork(state);
        }
      })
      .catch(() => {
        // An engine that does not answer leaves the bar as it is.
      });
    void onNetworkState((state) => {
      if (active) {
        setNetwork(state);
      }
    })
      .then((unlisten) => {
        if (active) {
          off = unlisten;
        } else {
          unlisten();
        }
      })
      .catch(() => {
        // An engine that does not answer leaves the bar as it is.
      });
    return () => {
      active = false;
      off?.();
    };
  }, []);

  const totalSpeed = visible.reduce((sum, item) => sum + item.speed, 0);
  const totalSize = visible.reduce((sum, item) => sum + item.size, 0);
  const active = downloads.filter((item) => item.status === "downloading").length;

  return (
    <footer className="flex h-7 shrink-0 items-stretch overflow-hidden bg-chrome-canvas text-chrome-micro text-chrome-muted">
      <span className="flex min-w-0 items-center px-3 tabular-nums">
        <span className="truncate">
          {visible.length} {t("status.downloads")} · {downloads.length}
        </span>
      </span>
      {active > 0 ? (
        <span className={`${CELL} hidden sm:flex`}>{active} {t("status.active")}</span>
      ) : null}
      {selected.length > 0 ? (
        <span className={CELL}>{selected.length} {t("status.selected")}</span>
      ) : null}
      {network && network.state !== "none" ? (
        <span
          className={CELL}
          role="status"
          title={
            network.state === "lost"
              ? "The chosen network adapter is not connected. Downloads stop."
              : `Every download leaves through ${network.label}.`
          }
        >
          {network.state === "lost" ? `${network.label}: lost` : network.label}
        </span>
      ) : null}
      {schedule && schedule.enabled ? (
        <span className={CELL} role="status">
          {schedule.open
            ? "queue: running"
            : schedule.next
              ? `queue runs ${schedule.next}`
              : "queue: scheduled"}
        </span>
      ) : null}
      <span className="ml-auto flex items-stretch">
        <span className={`${CELL} hidden sm:flex`}>
          {formatSize(totalSize)}
        </span>
        <span className={`${CELL} w-24 justify-end`}>
          {totalSpeed > 0 ? formatSpeed(totalSpeed) : t("status.idle")}
        </span>
      </span>
    </footer>
  );
}
