import { useRef } from "react";

import { cn } from "@/lib/utils";

export type ProgressTabId = "status" | "speed";

interface Tab {
  id: ProgressTabId;
  label: string;
}

/** The two pages of the window, in the order IDM puts them. */
const TABS: readonly Tab[] = [
  { id: "status", label: "Download status" },
  { id: "speed", label: "Speed limiter" },
];

interface ProgressTabsProps {
  active: ProgressTabId;
  onChange: (tab: ProgressTabId) => void;
}

/**
 * Two tabs over one panel.
 *
 * The window owns the panel, so the tabs only report which page it should
 * draw. Arrow keys walk the strip, Home and End go to its ends, and the tab
 * that is not selected leaves the Tab order, which is the pattern the ARIA
 * authoring practices describe for a tab list.
 */
export function ProgressTabs({ active, onChange }: ProgressTabsProps) {
  const refs = useRef<Array<HTMLButtonElement | null>>([]);

  function step(index: number, delta: number) {
    const next = (index + delta + TABS.length) % TABS.length;
    onChange(TABS[next].id);
    refs.current[next]?.focus();
  }

  return (
    <div
      role="tablist"
      aria-label="Download options"
      className="relative grid shrink-0 auto-cols-fr grid-flow-col self-start"
    >
      <span
        aria-hidden="true"
        style={{
          width: `${100 / TABS.length}%`,
          transform: `translateX(${TABS.findIndex((tab) => tab.id === active) * 100}%)`,
        }}
        className="pointer-events-none absolute inset-y-0 left-0 rounded-md bg-progress/12 transition-transform duration-300 ease-out motion-reduce:transition-none"
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
            id={`progress-tab-${tab.id}`}
            aria-selected={selected}
            aria-controls={`progress-panel-${tab.id}`}
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
              "relative rounded-md px-3 py-1.5 text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none",
              selected
                ? "font-medium text-chrome-menu-text"
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
