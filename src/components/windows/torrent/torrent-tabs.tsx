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
 * The chosen page is named by the text and a short accent rule under it, not
 * by a filled block: the tabs sit between the summary and the table, and a
 * slab of accent there would out-shout both. Arrow keys walk the strip, Home
 * and End go to its ends, and the unselected tabs leave the Tab order, which
 * is the pattern the ARIA authoring practices describe.
 */
export function TorrentTabs({ active, onChange }: TorrentTabsProps) {
  const refs = useRef<Array<HTMLButtonElement | null>>([]);

  function step(index: number, delta: number) {
    const next = (index + delta + TABS.length) % TABS.length;
    onChange(TABS[next].id);
    refs.current[next]?.focus();
  }

  return (
    <div role="tablist" aria-label="Torrent details" className="flex gap-7">
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
              "relative pb-2.5 text-[13px] transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none",
              selected
                ? "text-chrome-title-text"
                : "text-chrome-muted hover:text-chrome-menu-text",
            )}
          >
            {tab.label}
            <span
              aria-hidden="true"
              className={cn(
                "absolute inset-x-0 bottom-0 h-0.5 rounded-full transition-colors",
                selected ? "bg-chrome-focus" : "bg-transparent",
              )}
            />
          </button>
        );
      })}
    </div>
  );
}
