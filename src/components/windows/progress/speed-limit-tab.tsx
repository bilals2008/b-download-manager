import { useState } from "react";

import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { ListBox } from "@/components/ui/listbox";
import { Switch } from "@/components/ui/switch";
import { describeError } from "@/lib/errors";
import { setDownloadSpeedLimit } from "@/lib/ipc";
import {
  bytesToLimit,
  KB,
  limitLabel,
  limitToBytes,
  maxLimitAmount,
  parseLimitAmount,
  SPEED_UNITS,
  type SpeedUnit,
} from "@/lib/progress";
import type { DownloadInfo } from "@/types";

/** The cap a download starts with when the user turns the limit on. */
const DEFAULT_LIMIT_BYTES = 128 * KB;

interface SpeedLimitTabProps {
  info: DownloadInfo;
  /** The application-wide cap in bytes per second; 0 when it is off. */
  appLimit: number;
}

/**
 * Per-download speed cap. The engine reports the rate it actually holds, and
 * that is what "In force" shows.
 */
export function SpeedLimitTab({ info, appLimit }: SpeedLimitTabProps) {
  const opening = bytesToLimit(info.speedLimit > 0 ? info.speedLimit : DEFAULT_LIMIT_BYTES);
  const [enabled, setEnabled] = useState(info.speedLimit > 0);
  const [amountText, setAmountText] = useState(String(opening.amount));
  const [unit, setUnit] = useState<SpeedUnit>(opening.unit);
  const [applied, setApplied] = useState(info.speedLimit);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const amount = parseLimitAmount(amountText, unit);
  const nextBytes =
    enabled && amount !== null ? limitToBytes(amount, unit) : enabled ? null : 0;
  const dirty = nextBytes !== null && nextBytes !== applied;

  async function apply() {
    if (nextBytes === null) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const rate = await setDownloadSpeedLimit(info.id, nextBytes);
      setApplied(rate);
      setEnabled(rate > 0);
      if (rate === 0) {
        return;
      }
      const shown = bytesToLimit(rate);
      setAmountText(String(shown.amount));
      setUnit(shown.unit);
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setBusy(false);
    }
  }

  // A finished file has no bytes in flight, so there is nothing left to pace.
  if (info.status === "completed") {
    return (
      <section
        role="tabpanel"
        id="progress-panel-speed"
        aria-labelledby="progress-tab-speed"
        className="flex min-h-0 flex-1 flex-col gap-4"
      >
        <p className="text-xs text-chrome-menu-text">
          This download has finished
        </p>

        <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-6 gap-y-1.5 text-xs">
          <dt className="text-chrome-muted">In force</dt>
          <dd className="min-w-0 truncate text-right tabular-nums text-chrome-menu-text">
            {limitLabel(info.speedLimit)}
          </dd>
          <dt className="text-chrome-muted">Application limit</dt>
          <dd className="min-w-0 truncate text-right tabular-nums text-chrome-menu-text">
            {appLimit > 0 ? limitLabel(appLimit) : "No limit"}
          </dd>
        </dl>
      </section>
    );
  }

  return (
    <section
      role="tabpanel"
      id="progress-panel-speed"
      aria-labelledby="progress-tab-speed"
      className="flex min-h-0 flex-1 flex-col gap-4"
    >
      <div className="flex items-start justify-between gap-4">
        <p className="min-w-0 text-xs text-chrome-menu-text">
          Limit the speed of this download
        </p>
        <Switch
          label="Limit the speed of this download"
          checked={enabled}
          disabled={busy}
          onChange={(next) => {
            setEnabled(next);
            setError(null);
          }}
        />
      </div>

      <div className="flex items-end gap-2">
        <div className="min-w-0 flex-1">
          <label
            htmlFor="progress-speed-limit"
            className="block pb-1 text-chrome-micro text-chrome-muted"
          >
            Speed limit
          </label>
          <input
            id="progress-speed-limit"
            type="number"
            inputMode="numeric"
            min={1}
            max={maxLimitAmount(unit)}
            step={1}
            disabled={!enabled || busy}
            value={amountText}
            aria-invalid={enabled && amount === null}
            aria-describedby="progress-speed-limit-note"
            onChange={(event) => {
              setAmountText(event.target.value);
              setError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && dirty && nextBytes !== null) {
                event.preventDefault();
                void apply();
              }
            }}
            className={`${FIELD} tabular-nums`}
          />
        </div>

        <div className="w-24 shrink-0">
          <span className="block pb-1 text-chrome-micro text-chrome-muted">
            Unit
          </span>
          <ListBox
            label="Speed limit unit"
            options={SPEED_UNITS.map((value) => ({ value, label: value }))}
            value={unit}
            disabled={busy}
            onChange={(value) => {
              setUnit(value as SpeedUnit);
              setError(null);
            }}
          />
        </div>

        <DialogAction
          tone="primary"
          onClick={() => void apply()}
          disabled={!dirty || nextBytes === null || busy}
        >
          {busy ? "Applying…" : "Apply"}
        </DialogAction>
      </div>

      <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-6 gap-y-1.5 text-xs">
        <dt className="text-chrome-muted">In force</dt>
        <dd className="min-w-0 truncate text-right tabular-nums text-chrome-menu-text">
          {limitLabel(applied)}
        </dd>
        <dt className="text-chrome-muted">Application limit</dt>
        <dd className="min-w-0 truncate text-right tabular-nums text-chrome-menu-text">
          {appLimit > 0 ? limitLabel(appLimit) : "No limit"}
        </dd>
      </dl>

      <p
        id="progress-speed-limit-note"
        className="text-chrome-micro text-chrome-muted"
      >
        {appLimit > 0
          ? `The application limit of ${limitLabel(appLimit)} also applies; the lower wins.`
          : "Cap applies to every connection. Zero or off removes it."}
      </p>

      {error ? (
        <p role="alert" className="break-words text-chrome-micro text-chrome-error">
          {error}
        </p>
      ) : null}
    </section>
  );
}
