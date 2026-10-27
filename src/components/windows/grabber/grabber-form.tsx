import { FolderGlyph } from "@/components/icons/action-icons";
import { SpinnerGlyph } from "@/components/icons/ui-icons";
import { Checkbox } from "@/components/ui/checkbox";
import { DialogAction } from "@/components/ui/dialog-action";
import { FIELD } from "@/components/ui/field";
import { ListBox } from "@/components/ui/listbox";
import { Tooltip } from "@/components/ui/tooltip";
import {
  GRABBER_KINDS,
  MAX_DEPTH,
  MAX_PAGES,
  parseExtraExtensions,
  type GrabberKind,
  type GrabberPreferences,
} from "@/lib/grabber";
import { cn } from "@/lib/utils";

const LABEL = "mb-1.5 block text-xs text-chrome-muted";

const FOLDER_ROW =
  "group flex h-8 w-full min-w-0 items-center gap-2 rounded-chrome border border-chrome-line bg-chrome-canvas px-2 text-left text-xs transition-colors hover:border-chrome-muted focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none disabled:cursor-default disabled:opacity-55 disabled:hover:border-chrome-line";

interface GrabberFormProps {
  url: string;
  onUrlChange: (value: string) => void;
  problem: string | null;
  prefs: GrabberPreferences;
  onPrefsChange: (prefs: GrabberPreferences) => void;
  folderProblem: string | null;
  running: boolean;
  busy: boolean;
  onStart: () => void;
  onPickFolder: () => void;
}

/**
 * The left side of the window: one address, and every limit that keeps the
 * crawl from becoming a copy of the site.
 *
 * The form never starts a download. It starts a read, and the list on the
 * right is where the user decides what to keep.
 */
export function GrabberForm({
  url,
  onUrlChange,
  problem,
  prefs,
  onPrefsChange,
  folderProblem,
  running,
  busy,
  onStart,
  onPickFolder,
}: GrabberFormProps) {
  const extras = parseExtraExtensions(prefs.extensions);
  const hasTypes = prefs.types.length > 0 || extras.length > 0;
  const canStart =
    !!url.trim() &&
    !!prefs.folder &&
    !problem &&
    !folderProblem &&
    hasTypes &&
    !running &&
    !busy;

  function set<K extends keyof GrabberPreferences>(
    key: K,
    value: GrabberPreferences[K],
  ) {
    onPrefsChange({ ...prefs, [key]: value });
  }

  function toggleKind(kind: GrabberKind) {
    set(
      "types",
      prefs.types.includes(kind)
        ? prefs.types.filter((known) => known !== kind)
        : [...prefs.types, kind],
    );
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (canStart) {
          onStart();
        }
      }}
      className="flex w-[292px] shrink-0 flex-col border-r border-chrome-line"
    >
      <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-4">
        <div>
          <label htmlFor="grabber-url" className={LABEL}>
            Start address
          </label>
          <input
            id="grabber-url"
            autoFocus
            autoComplete="off"
            spellCheck={false}
            disabled={running}
            value={url}
            onChange={(event) => onUrlChange(event.target.value)}
            placeholder="https://example.org/files/"
            aria-invalid={problem ? true : undefined}
            aria-describedby="grabber-url-note"
            className={cn(FIELD, problem && "border-chrome-error/70")}
          />
          <p
            id="grabber-url-note"
            className={cn(
              "mt-1.5 text-chrome-micro",
              problem ? "text-chrome-error" : "text-chrome-muted",
            )}
          >
            {problem ?? "Only pages on this site are followed."}
          </p>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <span className={LABEL}>Follow links</span>
            <ListBox
              label="Follow links"
              disabled={running}
              value={String(prefs.depth)}
              onChange={(value) => set("depth", Number(value))}
              options={Array.from(
                { length: MAX_DEPTH },
                (_value, index) => index + 1,
              ).map((depth) => ({
                value: String(depth),
                label:
                  depth === 1
                    ? "1 · this page"
                    : `${depth} · ${depth - 1} step${depth === 2 ? "" : "s"} out`,
              }))}
            />
          </div>
          <div>
            <label htmlFor="grabber-pages" className={LABEL}>
              Pages to read
            </label>
            <input
              id="grabber-pages"
              type="number"
              min={1}
              max={MAX_PAGES}
              disabled={running}
              value={prefs.pages}
              onChange={(event) =>
                set(
                  "pages",
                  Math.min(MAX_PAGES, Math.max(1, Number(event.target.value) || 1)),
                )
              }
              className={cn(FIELD, "tabular-nums")}
            />
          </div>
        </div>
        <p className="-mt-3 text-chrome-micro text-chrome-muted">
          Reading stops at whichever comes first: the depth or the page count.
          A page is read once.
        </p>

        <fieldset>
          <legend className={LABEL}>File types</legend>
          <div className="grid grid-cols-2 gap-x-3 gap-y-1.5">
            {GRABBER_KINDS.map((kind) => (
              <Checkbox
                key={kind.id}
                label={kind.label}
                checked={prefs.types.includes(kind.id)}
                disabled={running}
                onChange={() => toggleKind(kind.id)}
              />
            ))}
          </div>
          <p className="mt-2 text-chrome-micro text-chrome-muted">
            Video playlists, web pages and page code are never offered as
            files.
          </p>
        </fieldset>

        <div>
          <label htmlFor="grabber-extensions" className={LABEL}>
            Extra types
          </label>
          <input
            id="grabber-extensions"
            autoComplete="off"
            spellCheck={false}
            disabled={running}
            value={prefs.extensions}
            onChange={(event) => set("extensions", event.target.value)}
            placeholder="iso, epub, bin"
            aria-describedby="grabber-extensions-note"
            className={FIELD}
          />
          <p
            id="grabber-extensions-note"
            className="mt-1.5 text-chrome-micro text-chrome-muted"
          >
            Extensions outside the groups, separated by commas.
          </p>
        </div>

        <div>
          <label htmlFor="grabber-exclude" className={LABEL}>
            Leave out
          </label>
          <input
            id="grabber-exclude"
            autoComplete="off"
            spellCheck={false}
            disabled={running}
            value={prefs.exclude}
            onChange={(event) => set("exclude", event.target.value)}
            placeholder="/private/*, /old/"
            aria-describedby="grabber-exclude-note"
            className={FIELD}
          />
          <p
            id="grabber-exclude-note"
            className="mt-1.5 text-chrome-micro text-chrome-muted"
          >
            Paths or patterns, separated by commas. <code>*</code> matches
            anything, and a path leaves out everything under it.
          </p>
        </div>

        <div>
          <span className={LABEL}>Save to</span>
          <Tooltip content={prefs.folder || "Choose a folder"} side="top">
            <button
              type="button"
              onClick={onPickFolder}
              disabled={running}
              className={FOLDER_ROW}
            >
              <FolderGlyph className="size-4 shrink-0 text-chrome-muted" />
              <span className="min-w-0 flex-1 truncate text-chrome-menu-text">
                {prefs.folder || "Choose a folder"}
              </span>
            </button>
          </Tooltip>
          {folderProblem ? (
            <p className="mt-1.5 text-chrome-micro text-chrome-error">
              {folderProblem}
            </p>
          ) : null}
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <span className={LABEL}>Queue</span>
            <ListBox
              label="Queue"
              disabled={running}
              value={prefs.queue}
              onChange={(value) =>
                set("queue", value === "main" ? "main" : "now")
              }
              options={[
                { value: "now", label: "Start now" },
                { value: "main", label: "Main queue" },
              ]}
            />
          </div>
          <div>
            <label htmlFor="grabber-connections" className={LABEL}>
              Connections
            </label>
            <input
              id="grabber-connections"
              type="number"
              min={1}
              max={32}
              disabled={running}
              value={prefs.connections}
              onChange={(event) =>
                set(
                  "connections",
                  Math.min(32, Math.max(1, Number(event.target.value) || 1)),
                )
              }
              aria-describedby="grabber-connections-note"
              className={cn(FIELD, "tabular-nums")}
            />
          </div>
        </div>
        <p id="grabber-connections-note" className="-mt-3 text-chrome-micro text-chrome-muted">
          Per file. The connection budget in Options still caps the whole
          application.
        </p>
      </div>

      <div className="shrink-0 border-t border-chrome-line p-4">
        <DialogAction
          type="submit"
          tone="primary"
          disabled={!canStart}
          className="w-full"
          icon={busy ? <SpinnerGlyph className="size-3.5 animate-spin" /> : undefined}
        >
          {busy ? "Starting…" : running ? "Reading…" : "Read site"}
        </DialogAction>
        <p className="mt-2 text-center text-chrome-micro text-chrome-muted">
          Nothing downloads until you pick the files.
        </p>
      </div>
    </form>
  );
}
