import { useEffect, useState } from "react";

import { onSettingsChanged } from "@/lib/app-events";
import { loadSettings } from "@/lib/settings";

/** The application-wide cap in bytes per second; 0 when it is off. */
function readAppSpeedLimit(): number {
  const settings = loadSettings();
  return settings.speedLimitEnabled ? settings.speedLimitBytes : 0;
}

/**
 * The application-wide speed limit, kept in step with the settings window.
 *
 * The limit belongs to Options → Connection, not to a download. The progress
 * window only reports it, but it has to report the value in force now, not the
 * one that was in force when the window opened.
 */
export function useAppSpeedLimit(): number {
  const [limit, setLimit] = useState(readAppSpeedLimit);

  useEffect(() => {
    const read = () => setLimit(readAppSpeedLimit());
    const stop = onSettingsChanged(read);
    // `localStorage` is shared by the windows; the storage event is the second
    // way to hear a change when the event above is not delivered.
    window.addEventListener("storage", read);
    return () => {
      stop();
      window.removeEventListener("storage", read);
    };
  }, []);

  return limit;
}
