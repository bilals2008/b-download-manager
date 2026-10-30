import { useRef, useState } from "react";

import { Dialog } from "@/components/ui/dialog";
import { DialogAction } from "@/components/ui/dialog-action";
import type { ToolbarPack } from "@/lib/toolbar-packs";
import {
  MAX_TOOLBAR_TOKENS,
  TOOLBAR_CATALOG,
  TOOLBAR_SEPARATOR,
  isToolbarCommand,
  normalizeToolbarLayout,
  type ToolbarCommand,
  type ToolbarToken,
} from "@/lib/toolbars";

interface CustomizeToolbarDialogProps {
  /** The pack being customized; its name titles the window. */
  pack: ToolbarPack;
  /** The layout in use right now. */
  layout: readonly ToolbarToken[];
  /** What Reset returns to: the pack's own suggestion, or the default bar. */
  defaultLayout: readonly ToolbarToken[];
  onSave: (layout: ToolbarToken[]) => void;
  onClose: () => void;
}

interface Option {
  key: string;
  label: string;
  muted?: boolean;
}

/**
 * Move buttons between "available" and "on this toolbar", and put them in the
 * order they should be drawn. Separators group them. The layout cannot lose
 * its last button: an empty bar has nothing to press.
 */
export function CustomizeToolbarDialog({
  pack,
  layout,
  defaultLayout,
  onSave,
  onClose,
}: CustomizeToolbarDialogProps) {
  const [draft, setDraft] = useState<ToolbarToken[]>(() => [...layout]);
  const [availableIndex, setAvailableIndex] = useState<number | null>(0);
  const [currentIndex, setCurrentIndex] = useState<number | null>(
    layout.length > 0 ? 0 : null,
  );

  const used = new Set(
    draft.filter((token): token is ToolbarCommand => isToolbarCommand(token)),
  );
  const available = TOOLBAR_CATALOG.filter((item) => !used.has(item.command));
  const safeAvailable =
    available.length === 0
      ? null
      : Math.min(availableIndex ?? 0, available.length - 1);
  const safeCurrent =
    currentIndex !== null && currentIndex < draft.length ? currentIndex : null;

  const commandCount = draft.filter((token) => token !== TOOLBAR_SEPARATOR).length;
  const selected = safeCurrent === null ? undefined : draft[safeCurrent];
  const canRemove =
    selected !== undefined &&
    (selected === TOOLBAR_SEPARATOR || commandCount > 1);
  const resetLocked = sameLayout(draft, defaultLayout);

  const availableOptions: Option[] = available.map((item) => ({
    key: item.command,
    label: item.label,
  }));
  const currentOptions: Option[] = draft.map((token, index) => ({
    key: `${token}-${index}`,
    label:
      token === TOOLBAR_SEPARATOR
        ? "Separator"
        : (TOOLBAR_CATALOG.find((item) => item.command === token)?.label ?? token),
    muted: token === TOOLBAR_SEPARATOR,
  }));

  function addCommand(index: number) {
    const item = available[index];
    if (!item) {
      return;
    }
    insert(item.command);
    setAvailableIndex(0);
  }

  function insert(token: ToolbarToken) {
    if (draft.length >= MAX_TOOLBAR_TOKENS) {
      return;
    }
    const at = safeCurrent === null ? draft.length : safeCurrent + 1;
    const next = [...draft.slice(0, at), token, ...draft.slice(at)];
    setDraft(next);
    setCurrentIndex(at);
  }

  function removeSelected() {
    if (safeCurrent === null || !canRemove) {
      return;
    }
    const next = draft.filter((_, index) => index !== safeCurrent);
    setDraft(next);
    setCurrentIndex(next.length === 0 ? null : Math.min(safeCurrent, next.length - 1));
  }

  function moveSelected(delta: number) {
    if (safeCurrent === null) {
      return;
    }
    const target = safeCurrent + delta;
    if (target < 0 || target >= draft.length) {
      return;
    }
    const next = [...draft];
    [next[safeCurrent], next[target]] = [next[target], next[safeCurrent]];
    setDraft(next);
    setCurrentIndex(target);
  }

  function reset() {
    setDraft([...defaultLayout]);
    setCurrentIndex(defaultLayout.length > 0 ? 0 : null);
  }

  function save() {
    onSave(normalizeToolbarLayout(draft, defaultLayout));
    onClose();
  }

  return (
    <Dialog
      open
      title={pack.name === "Default" ? "Customize Toolbar" : `Customize ${pack.name} Toolbar`}
      onClose={onClose}
      className="max-h-[calc(100dvh-2rem)] w-[640px]"
      bodyClassName="overflow-y-auto"
      footer={
        <>
          <DialogAction
            tone="quiet"
            className="mr-auto"
            onClick={reset}
            disabled={resetLocked}
          >
            Reset
          </DialogAction>
          <DialogAction onClick={onClose}>Cancel</DialogAction>
          <DialogAction tone="primary" data-autofocus onClick={save}>
            OK
          </DialogAction>
        </>
      }
    >
      <p className="pb-3 text-xs text-chrome-muted">
        Choose which buttons this toolbar shows, and in what order. Double-click
        a button to move it across.
      </p>

      <div className="grid grid-cols-2 gap-3">
        <div className="flex min-w-0 flex-col gap-1.5">
          <span className="text-xs text-chrome-muted">Available buttons</span>
          <OptionList
            label="Available toolbar buttons"
            items={availableOptions}
            selected={safeAvailable}
            onSelect={setAvailableIndex}
            onActivate={addCommand}
            empty="Every button is on the toolbar."
          />
        </div>
        <div className="flex min-w-0 flex-col gap-1.5">
          <span className="text-xs text-chrome-muted">Buttons on this toolbar</span>
          <OptionList
            label="Buttons on this toolbar"
            items={currentOptions}
            selected={safeCurrent}
            onSelect={setCurrentIndex}
            onDelete={removeSelected}
            empty="No buttons yet."
          />
        </div>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <DialogAction
          onClick={() => {
            if (safeAvailable !== null) {
              addCommand(safeAvailable);
            }
          }}
          disabled={safeAvailable === null}
        >
          Add
        </DialogAction>
        <DialogAction
          onClick={() => insert(TOOLBAR_SEPARATOR)}
          disabled={draft.length >= MAX_TOOLBAR_TOKENS}
        >
          Add separator
        </DialogAction>
        <span className="flex-1" />
        <DialogAction onClick={removeSelected} disabled={!canRemove}>
          Remove
        </DialogAction>
        <DialogAction
          onClick={() => moveSelected(-1)}
          disabled={safeCurrent === null || safeCurrent === 0}
        >
          Move up
        </DialogAction>
        <DialogAction
          onClick={() => moveSelected(1)}
          disabled={safeCurrent === null || safeCurrent === draft.length - 1}
        >
          Move down
        </DialogAction>
      </div>
    </Dialog>
  );
}

function sameLayout(
  left: readonly ToolbarToken[],
  right: readonly ToolbarToken[],
): boolean {
  return left.length === right.length && left.every((token, index) => token === right[index]);
}

interface OptionListProps {
  label: string;
  items: Option[];
  selected: number | null;
  onSelect: (index: number) => void;
  /** Enter or a double click; used by the available list to add a button. */
  onActivate?: (index: number) => void;
  /** Delete; used by the current list to take a button off the bar. */
  onDelete?: () => void;
  empty: string;
}

/** One list of a transfer pair, navigable with the arrow keys. */
function OptionList({
  label,
  items,
  selected,
  onSelect,
  onActivate,
  onDelete,
  empty,
}: OptionListProps) {
  const refs = useRef<Array<HTMLButtonElement | null>>([]);

  function focus(index: number) {
    if (items.length === 0) {
      return;
    }
    const next = (index + items.length) % items.length;
    onSelect(next);
    refs.current[next]?.focus();
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLButtonElement>, index: number) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focus(index + 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        focus(index - 1);
        break;
      case "Home":
        event.preventDefault();
        focus(0);
        break;
      case "End":
        event.preventDefault();
        focus(items.length - 1);
        break;
      case "Enter":
        if (onActivate) {
          event.preventDefault();
          onActivate(index);
        }
        break;
      case "Delete":
        if (onDelete) {
          event.preventDefault();
          onDelete();
        }
        break;
    }
  }

  return (
    <ul
      role="listbox"
      aria-label={label}
      className="h-60 overflow-y-auto rounded-chrome border border-chrome-line bg-chrome-canvas p-1"
    >
      {items.length === 0 ? (
        <li className="px-2 py-1.5 text-xs text-chrome-muted">{empty}</li>
      ) : null}
      {items.map((item, index) => {
        const isSelected = index === selected;
        return (
          <li key={item.key} role="none">
            <button
              ref={(node) => {
                refs.current[index] = node;
              }}
              type="button"
              role="option"
              aria-selected={isSelected}
              tabIndex={isSelected || (selected === null && index === 0) ? 0 : -1}
              onClick={() => onSelect(index)}
              onDoubleClick={() => onActivate?.(index)}
              onFocus={() => onSelect(index)}
              onKeyDown={(event) => handleKeyDown(event, index)}
              className={
                "flex w-full items-center rounded-chrome px-2 py-1 text-left text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
                (item.muted ? "text-chrome-muted " : "text-chrome-menu-text ") +
                (isSelected
                  ? "bg-chrome-menu-active"
                  : "hover:bg-chrome-menu-hover")
              }
            >
              <span className="truncate">{item.label}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
