import { useRef } from "react";

import {
  TorrentTabGlyph,
  type TorrentGlyphKind,
} from "@/components/icons/ui-icons";
import { cn } from "@/lib/utils";

export type TorrentTabId = TorrentGlyphKind;

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
 * The chosen page is a soft chip with its mark in the accent, not an underline
 * under plain text: the tabs sit between the summary and the table, and a tint
 * says which page is open without drawing a rule across the window. Arrow
 * keys walk the strip, Home and End go to its ends, and the unselected tabs
 * leave the Tab order, which is the pattern the ARIA authoring practices
 * describe.
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
      className="flex w-fit max-w-full gap-0.5 overflow-x-auto rounded-chrome bg-chrome-menu p-1 scrollbar-thin"
    >
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
              "flex items-center gap-1.5 rounded-chrome px-2.5 py-1 text-[13px] transition-colors duration-150 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
              selected
                ? "bg-chrome-menu-active text-chrome-title-text"
                : "text-chrome-muted hover:bg-chrome-menu-hover hover:text-chrome-menu-text",
            )}
          >
            <TorrentTabGlyph
              kind={tab.id}
              className={cn(
                "size-3.5 shrink-0 transition-colors duration-150",
                selected ? "text-chrome-focus" : "opacity-70",
              )}
            />
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}
