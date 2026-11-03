import {
  useDeferredValue,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";

import {
  CloseGlyph,
  SearchGlyph,
  SettingsGlyph,
  type SettingsGlyphKind,
} from "@/components/icons/ui-icons";
import { SETTINGS_PAGES } from "@/components/settings/settings-nav";
import {
  groupSettingsCatalogHits,
  searchSettingsCatalog,
  type SettingsCatalogEntry,
} from "@/components/settings/settings-search-data";
import { searchTokens } from "@/lib/settings-search";
import { cn } from "@/lib/utils";

const PAGE_GLYPHS: ReadonlyMap<string, SettingsGlyphKind> = new Map(
  SETTINGS_PAGES.map((page) => [page.id, page.glyph]),
);

interface SettingsSearchProps {
  /** The page the sidebar has open; changing it abandons the search. */
  page: string;
  /** Open the page and row the chosen result stands for. */
  onNavigate: (entry: SettingsCatalogEntry) => void;
  /** The page that is shown while the query is empty. */
  children: ReactNode;
}

/**
 * The search field and its results panel.
 *
 * The index is prepared at module load, so typing only filters a small array:
 * there is no request to wait for and no reason for a spinner or a skeleton.
 * The query is deferred so a slow render never holds the field behind it, and
 * every keystroke still lands in the input immediately.
 */
export function SettingsSearch({
  page,
  onNavigate,
  children,
}: SettingsSearchProps) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const deferred = useDeferredValue(query);
  const inputRef = useRef<HTMLInputElement>(null);
  // The arrow key and the Enter that follows it can arrive before React has
  // drawn the last move. The ref carries the newest position so the second
  // key opens the row the reader is looking at.
  const activeRef = useRef(0);

  // A page change from the sidebar is a new destination: the results panel
  // must give way to it, or the selected page would stay hidden behind old
  // results. The state is derived here, so no effect runs after the paint.
  const [shownPage, setShownPage] = useState(page);
  if (shownPage !== page) {
    setShownPage(page);
    if (query) {
      setQuery("");
      setActive(0);
      activeRef.current = 0;
    }
  }

  const hits = useMemo(
    () => searchSettingsCatalog(deferred),
    [deferred],
  );
  const groups = useMemo(() => groupSettingsCatalogHits(hits), [hits]);
  const flat = useMemo(() => groups.flatMap((group) => group.hits), [groups]);
  const tokens = useMemo(() => searchTokens(deferred), [deferred]);

  // The field decides when the results panel is shown, not the deferred copy:
  // clearing the query must bring the page back at once. The deferred value is
  // only used to filter, so the field itself stays ahead of a slow render.
  const activeQuery = query.trim();
  const searching = activeQuery.length > 0;
  const activeIndex =
    flat.length === 0 ? -1 : Math.min(active, flat.length - 1);

  function move(index: number) {
    activeRef.current = index;
    setActive(index);
  }

  function clear() {
    setQuery("");
    move(0);
    inputRef.current?.focus();
  }

  function choose(entry: SettingsCatalogEntry) {
    onNavigate(entry);
    setQuery("");
    move(0);
  }

  function moveActive(delta: number) {
    if (flat.length === 0) {
      return;
    }
    const next =
      (Math.min(activeRef.current, flat.length - 1) + delta + flat.length) %
      flat.length;
    move(next);
    // The result may be below the fold; keep the active row visible without
    // waiting for a state render.
    window.requestAnimationFrame(() => {
      document
        .getElementById(`settings-result-${next}`)
        ?.scrollIntoView?.({ block: "nearest" });
    });
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (!searching) {
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveActive(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      moveActive(-1);
      return;
    }
    if (event.key === "Enter") {
      const hit = flat[Math.min(activeRef.current, flat.length - 1)];
      if (hit) {
        event.preventDefault();
        choose(hit.entry);
      }
      return;
    }
    if (event.key === "Escape") {
      // The first Escape clears the search; the dialog must stay open.
      event.preventDefault();
      event.stopPropagation();
      clear();
    }
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="shrink-0 px-6 pt-4 pb-3">
        <div className="relative">
          <SearchGlyph className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-chrome-muted" />
          <input
            ref={inputRef}
            data-autofocus
            type="search"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              move(0);
            }}
            onKeyDown={onKeyDown}
            placeholder="Search settings"
            aria-label="Search settings"
            role="combobox"
            aria-autocomplete="list"
            aria-expanded={searching}
            aria-controls={
              searching && flat.length > 0 ? "settings-search-results" : undefined
            }
            aria-activedescendant={
              searching && activeIndex >= 0
                ? `settings-result-${activeIndex}`
                : undefined
            }
            autoComplete="off"
            spellCheck={false}
            className={cn(
              "h-9 w-full rounded-chrome bg-chrome-menu-hover pr-10 pl-9 text-xs text-chrome-menu-text outline-none transition-colors",
              "placeholder:text-chrome-muted",
              "focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset",
              "[&::-webkit-search-cancel-button]:appearance-none",
            )}
          />
          {query ? (
            <button
              type="button"
              aria-label="Clear search"
              onClick={clear}
              className="absolute top-1/2 right-1.5 flex size-6 -translate-y-1/2 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none"
            >
              <CloseGlyph className="size-3" />
            </button>
          ) : null}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto scrollbar-thin">
        {searching ? (
          <div className="px-6 pt-1 pb-6">
            {flat.length === 0 ? (
              <div className="flex flex-col items-center gap-1 py-14 text-center">
                <p className="text-xs text-chrome-title-text">
                  No setting matches “{activeQuery}”.
                </p>
                <p className="max-w-72 text-chrome-micro leading-4 text-chrome-muted">
                  Try a shorter word, or a synonym like speed, folder or
                  shortcut.
                </p>
              </div>
            ) : (
              <>
                <p
                  role="status"
                  className="pb-2 text-chrome-micro text-chrome-muted"
                >
                  {flat.length} {flat.length === 1 ? "result" : "results"}
                </p>
                <div
                  id="settings-search-results"
                  role="listbox"
                  aria-label="Settings results"
                  className="flex flex-col gap-3"
                >
                  {groups.map((group) => (
                    <div
                      key={group.page}
                      role="group"
                      aria-labelledby={`settings-group-${group.page}`}
                      className="flex flex-col"
                    >
                      <p
                        id={`settings-group-${group.page}`}
                        className="flex items-center gap-1.5 px-2 pb-1 text-chrome-micro font-medium text-chrome-muted"
                      >
                        <SettingsGlyph
                          kind={PAGE_GLYPHS.get(group.page) ?? "general"}
                          className="size-3.5 shrink-0"
                        />
                        {group.label}
                      </p>
                      {group.hits.map((hit) => {
                        const isActive = hit.index === activeIndex;
                        return (
                          <button
                            key={hit.entry.id}
                            id={`settings-result-${hit.index}`}
                            type="button"
                            role="option"
                            tabIndex={-1}
                            aria-selected={isActive}
                            onMouseEnter={() => move(hit.index)}
                            onClick={() => choose(hit.entry)}
                            className={cn(
                              "flex items-start gap-3 rounded-chrome px-2 py-1.5 text-left transition-colors focus-visible:outline-none",
                              isActive
                                ? "bg-chrome-menu-active"
                                : "hover:bg-chrome-menu-hover",
                            )}
                          >
                            <span className="min-w-0 flex-1">
                              <span className="block truncate text-xs text-chrome-menu-text">
                                <Highlighted
                                  text={hit.entry.title}
                                  tokens={tokens}
                                />
                              </span>
                              {hit.entry.description ? (
                                <span className="block truncate text-chrome-micro text-chrome-muted">
                                  {hit.entry.description}
                                </span>
                              ) : null}
                            </span>
                            <span className="shrink-0 pt-0.5 text-chrome-micro text-chrome-muted">
                              {hit.entry.section}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  ))}
                </div>
              </>
            )}
          </div>
        ) : (
          children
        )}
      </div>
    </div>
  );
}

const REGEX_SPECIAL = /[.*+?^${}()|[\]\\]/g;

function escapeRegExp(value: string): string {
  return value.replace(REGEX_SPECIAL, "\\$&");
}

/** Draw the words that matched in the accent, without changing the text. */
function Highlighted({
  text,
  tokens,
}: {
  text: string;
  tokens: readonly string[];
}) {
  if (tokens.length === 0) {
    return <>{text}</>;
  }
  const pattern = new RegExp(
    `(${tokens.map(escapeRegExp).join("|")})`,
    "gi",
  );
  const words = new Set(tokens);
  return (
    <>
      {text.split(pattern).map((part, index) =>
        words.has(part.toLocaleLowerCase()) ? (
          <mark
            key={`${part}-${index}`}
            className="bg-transparent font-medium text-chrome-focus"
          >
            {part}
          </mark>
        ) : (
          part
        ),
      )}
    </>
  );
}
