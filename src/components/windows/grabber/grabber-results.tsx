import { useState } from "react";

import { CheckGlyph, SearchGlyph } from "@/components/icons/ui-icons";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { ListBox } from "@/components/ui/listbox";
import { Tooltip } from "@/components/ui/tooltip";
import {
  filePath,
  GRABBER_KINDS,
  KIND_LABELS,
  notStarted,
  resultSummary,
  scanSummary,
  shortAddress,
  type GrabberFile,
  type GrabberItemResult,
  type GrabberKind,
  type GrabberNote,
  type GrabberSnapshot,
} from "@/lib/grabber";
import { cn } from "@/lib/utils";

interface GrabberResultsProps {
  snapshot: GrabberSnapshot;
  /** The files the filters show. */
  files: GrabberFile[];
  /** Every file the crawl found, filtered or not. */
  totalFiles: number;
  kind: GrabberKind | "all";
  onKindChange: (kind: GrabberKind | "all") => void;
  query: string;
  onQueryChange: (query: string) => void;
  selected: ReadonlySet<string>;
  sent: ReadonlySet<string>;
  onToggle: (url: string, checked: boolean) => void;
  onSelectShown: () => void;
  onClear: () => void;
  results: GrabberItemResult[];
}

type Tab = "files" | "log";

/**
 * The right side of the window: what the crawl found, and what it left out.
 *
 * Files start unticked on purpose. A grabber that pre-selects everything turns
 * "preview the links" into one click that downloads a site, which is the thing
 * this window is meant to make avoidable.
 */
export function GrabberResults({
  snapshot,
  files,
  totalFiles,
  kind,
  onKindChange,
  query,
  onQueryChange,
  selected,
  sent,
  onToggle,
  onSelectShown,
  onClear,
  results,
}: GrabberResultsProps) {
  const [tab, setTab] = useState<Tab>("files");
  const failures = notStarted(results);
  const logCount = snapshot.notes.length + failures.length;
  const idle =
    snapshot.status === "idle" && totalFiles === 0 && snapshot.notes.length === 0;

  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-chrome-line px-3 py-2">
        <div role="tablist" aria-label="Grabber results" className="flex gap-1">
          <TabButton
            active={tab === "files"}
            onClick={() => setTab("files")}
            count={totalFiles}
          >
            Files
          </TabButton>
          <TabButton
            active={tab === "log"}
            onClick={() => setTab("log")}
            count={logCount}
          >
            Log
          </TabButton>
        </div>

        {tab === "files" ? (
          <div className="ml-auto flex min-w-0 items-center gap-2">
            <div className="relative">
              <SearchGlyph className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-chrome-muted" />
              <input
                type="search"
                value={query}
                onChange={(event) => onQueryChange(event.target.value)}
                placeholder="Filter files"
                aria-label="Filter files"
                className={cn(FIELD, "h-7 w-40 py-1 pl-7")}
              />
            </div>
            <div className="w-28">
              <ListBox
                label="Show one file type"
                value={kind}
                onChange={(value) => onKindChange(value as GrabberKind | "all")}
                options={[
                  { value: "all", label: "All types" },
                  ...GRABBER_KINDS.map((entry) => ({
                    value: entry.id,
                    label: entry.label,
                  })),
                ]}
              />
            </div>
          </div>
        ) : null}
      </div>

      {tab === "files" ? (
        <>
          <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-chrome-line px-4 py-2">
            <span className="text-chrome-micro text-chrome-muted">
              {scanSummary(snapshot)}
            </span>
            <div className="ml-auto flex items-center gap-1">
              <DialogAction
                tone="quiet"
                disabled={!files.length}
                onClick={onSelectShown}
              >
                Select shown
              </DialogAction>
              <DialogAction tone="quiet" disabled={!selected.size} onClick={onClear}>
                Clear
              </DialogAction>
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto">
            {files.length ? (
              <ul className="py-1">
                {files.map((file) => (
                  <FileRow
                    key={file.url}
                    file={file}
                    checked={selected.has(file.url) || sent.has(file.url)}
                    disabled={sent.has(file.url)}
                    sent={sent.has(file.url)}
                    onToggle={onToggle}
                  />
                ))}
              </ul>
            ) : (
              <EmptyState
                title={
                  idle
                    ? "Nothing read yet"
                    : snapshot.status === "running" ||
                        snapshot.status === "cancelling"
                      ? "Reading…"
                      : totalFiles
                        ? "No file matches this filter"
                        : "No file found"
                }
                body={
                  idle
                    ? "Enter a page address, choose the types you want, and read the site. The files it offers appear here."
                    : snapshot.status === "running" ||
                        snapshot.status === "cancelling"
                      ? "Files appear as the pages are read. Nothing downloads until you choose it."
                      : totalFiles
                        ? "Clear the filter or choose another type to see the rest."
                        : "The pages were read, but none of them linked to a file of the chosen types. The log says what was left out."
                }
              />
            )}
          </div>

          {results.length ? (
            <div className="flex shrink-0 items-center gap-2 border-t border-chrome-line px-4 py-2 text-chrome-micro text-chrome-muted">
              <span className="min-w-0 flex-1 truncate">
                {resultSummary(results)}
              </span>
              {failures.length ? (
                <button
                  type="button"
                  onClick={() => setTab("log")}
                  className="shrink-0 rounded-chrome px-1.5 py-0.5 text-chrome-error transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none"
                >
                  {failures.length} to review
                </button>
              ) : null}
            </div>
          ) : null}
        </>
      ) : (
        <Log
          notes={snapshot.notes}
          omitted={snapshot.notesOmitted}
          failures={failures}
        />
      )}
    </section>
  );
}

function TabButton({
  active,
  count,
  onClick,
  children,
}: {
  active: boolean;
  count: number;
  onClick: () => void;
  children: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cn(
        "flex h-7 items-center gap-1.5 rounded-chrome px-2 text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
        active
          ? "bg-chrome-menu-active text-chrome-menu-text"
          : "text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text",
      )}
    >
      {children}
      {count > 0 ? (
        <span className="tabular-nums text-chrome-micro text-chrome-muted">
          {count}
        </span>
      ) : null}
    </button>
  );
}

function FileRow({
  file,
  checked,
  disabled,
  sent,
  onToggle,
}: {
  file: GrabberFile;
  checked: boolean;
  disabled: boolean;
  sent: boolean;
  onToggle: (url: string, checked: boolean) => void;
}) {
  return (
    <li
      className={cn(
        "group flex items-center gap-3 px-4 py-1.5 transition-colors",
        disabled ? "opacity-60" : "hover:bg-chrome-row-alt",
        checked && !disabled && "bg-chrome-row-alt/70",
      )}
    >
      <span className="relative flex size-4 shrink-0 items-center justify-center">
        <input
          type="checkbox"
          checked={checked}
          disabled={disabled}
          onChange={(event) => onToggle(file.url, event.target.checked)}
          aria-label={file.name}
          className="peer size-4 appearance-none rounded-sm border border-input bg-chrome-canvas transition-colors hover:border-chrome-muted checked:border-primary checked:bg-primary disabled:cursor-default disabled:border-chrome-line disabled:checked:bg-chrome-line/60"
        />
        <CheckGlyph className="pointer-events-none absolute size-3 text-primary-foreground opacity-0 peer-checked:opacity-100 peer-disabled:text-chrome-muted" />
      </span>
      <span className="w-10 shrink-0 rounded-sm border border-chrome-line py-0.5 text-center text-chrome-micro uppercase tabular-nums text-chrome-muted">
        {file.extension || "file"}
      </span>
      <Tooltip content={file.name} side="top" className="min-w-0 flex-1">
        <span className="block truncate text-xs text-chrome-menu-text">
          {file.name}
        </span>
      </Tooltip>
      <Tooltip
        content={file.url}
        side="top"
        className="hidden min-w-0 max-w-[42%] sm:block"
      >
        <span className="block truncate font-mono text-chrome-micro text-chrome-muted">
          {filePath(file.url)}
        </span>
      </Tooltip>
      <span className="w-16 shrink-0 text-right text-chrome-micro text-chrome-muted">
        {sent ? "sent" : KIND_LABELS[file.kind]}
      </span>
    </li>
  );
}

function Log({
  notes,
  omitted,
  failures,
}: {
  notes: GrabberNote[];
  omitted: number;
  failures: GrabberItemResult[];
}) {
  const empty = notes.length === 0 && failures.length === 0;

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-4 py-2">
      {empty ? (
        <EmptyState
          title="Nothing left out"
          body="Blocked pages, robots.txt refusals and files the app would not take are listed here."
        />
      ) : (
        <ul className="space-y-1.5 py-1">
          {notes.map((note, index) => (
            <li
              key={`note-${index}`}
              className="grid grid-cols-[auto_1fr] items-baseline gap-x-2"
            >
              <span
                aria-hidden="true"
                className={cn(
                  "mt-1 size-1.5 rounded-full",
                  note.level === "blocked"
                    ? "bg-chrome-danger"
                    : note.level === "error"
                      ? "bg-chrome-error"
                      : "bg-chrome-muted",
                )}
              />
              <div className="min-w-0">
                <p className="text-xs break-words text-chrome-menu-text">
                  {note.message}
                </p>
                {note.url ? (
                  <p className="font-mono text-chrome-micro break-all text-chrome-muted">
                    {shortAddress(note.url)}
                  </p>
                ) : null}
              </div>
            </li>
          ))}

          {failures.map((result, index) => (
            <li
              key={`result-${index}`}
              className="grid grid-cols-[auto_1fr] items-baseline gap-x-2"
            >
              <span
                aria-hidden="true"
                className={cn(
                  "mt-1 size-1.5 rounded-full",
                  result.status === "duplicate"
                    ? "bg-chrome-muted"
                    : "bg-chrome-error",
                )}
              />
              <div className="min-w-0">
                <p className="text-xs break-words text-chrome-menu-text">
                  {result.message ||
                    (result.status === "duplicate"
                      ? "Already in the download list."
                      : "The app did not take it.")}
                </p>
                <p className="font-mono text-chrome-micro break-all text-chrome-muted">
                  {result.name || shortAddress(result.url)}
                </p>
              </div>
            </li>
          ))}
        </ul>
      )}

      {omitted > 0 ? (
        <p className="pt-2 text-chrome-micro text-chrome-muted">
          {omitted} more line{omitted === 1 ? "" : "s"} were not kept.
        </p>
      ) : null}
    </div>
  );
}

function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="grid h-full place-items-center px-8 py-10">
      <div className="max-w-sm text-center">
        <p className="text-xs font-medium text-chrome-menu-text">{title}</p>
        <p className="mt-1 text-chrome-micro leading-5 text-chrome-muted">
          {body}
        </p>
      </div>
    </div>
  );
}
