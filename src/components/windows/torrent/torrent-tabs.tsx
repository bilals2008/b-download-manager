import { useRef } from "react";

import { cn } from "@/lib/utils";

export type TorrentTabId = "overview" | "files" | "pieces" | "peers" | "trackers";

interface Tab {
  id: TorrentTabId;
  label: string;
}

/** The five pages of a job, the summary one first. */
const TABS: readonly Tab[] = [
  { id: "overview", label: "Overview" },
  { id: "files", label: "Files" },
  { id: "pieces", label: "Pieces" },
  { id: "peers", label: "Peers" },
  { id: "trackers", label: "Trackers" },
];

interface TorrentTabsProps {
  active: TorrentTabId;
  onChange: (tab: TorrentTabId) => void;
}

/**
 * Five tabs over one panel.
 *
 * The window owns the panel, so the tabs only report which page it should
 * draw. Arrow keys walk the strip, Home and End go to its ends, and the tab
 * that is not selected leaves the Tab order, which is the pattern the ARIA
 * authoring practices describe for a tab list.
 */
export function TorrentTabs({ active, onChange }: TorrentTabsProps) {
  const refs = useRef<Array<HTMLButtonElement | null>>([]);

  function step(index: number, delta: number) {
    const next = (index + delta + TABS.length) % TABS.length;
    onChange(TABS[next].id);
    refs.current[next]?.focus();
  }

  return (
    <div
      role="tablist"
      aria-label="Torrent details"
      className="relative grid w-full max-w-md shrink-0 auto-cols-fr grid-flow-col self-start"
    >
      <span
        aria-hidden="true"
        style={{
          width: `${100 / TABS.length}%`,
          transform: `translateX(${TABS.findIndex((tab) => tab.id === active) * 100}%)`,
        }}
        className="pointer-events-none absolute inset-y-0 left-0 rounded-chrome bg-primary transition-transform duration-300 ease-out motion-reduce:transition-none"
      />
      {TABS.map((tab, index) => {
        const selected = tab.id === active;
        return (
          <button
            key={tab.id}
            ref={(node) => {
              refs.current[index] = node;
            }}
            type="button"
            role="tab"
            id={`torrent-tab-${tab.id}`}
            aria-selected={selected}
            aria-controls={`torrent-panel-${tab.id}`}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(tab.id)}
            onKeyDown={(event) => {
              if (event.key === "ArrowRight") {
                event.preventDefault();
                step(index, 1);
              } else if (event.key === "ArrowLeft") {
                event.preventDefault();
                step(index, -1);
              } else if (event.key === "Home") {
                event.preventDefault();
                step(index, -index);
              } else if (event.key === "End") {
                event.preventDefault();
                step(index, TABS.length - 1 - index);
              }
            }}
            className={cn(
              "relative truncate rounded-chrome px-3 py-1.5 text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none",
              selected
                ? "font-medium text-primary-foreground"
                : "text-chrome-muted hover:text-chrome-menu-text",
            )}
          >
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}
