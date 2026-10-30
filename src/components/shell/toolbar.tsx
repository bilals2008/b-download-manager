import { useRef, useState, type CSSProperties, type ReactNode } from "react";

import { Tooltip } from "@/components/ui/tooltip";
import type { ToolbarSize } from "@/lib/settings";
import {
  toolbarPackStyleVars,
  type ToolbarPack,
} from "@/lib/toolbar-packs";
import { TOOLBAR_SEPARATOR, type ToolbarToken } from "@/lib/toolbars";

import type { ShellCommand } from "./commands";
import { ToolbarPackIcon } from "./toolbar-icon";
import { TOOLBAR_BY_COMMAND, type ToolbarItem } from "./toolbar-items";
import type { ToolbarDisabled } from "./toolbar-state";

interface ToolbarProps {
  onCommand: (command: ShellCommand) => void;
  /** Large buttons are the default; small ones make room for the list. */
  size: ToolbarSize;
  /** The buttons of the pack in use, in the order the reader chose. */
  layout: readonly ToolbarToken[];
  /** The pack that paints the bar and may bring its own icons. */
  pack: ToolbarPack;
  /** Whether each button prints its name under the icon. */
  showLabels?: boolean;
  /** Buttons whose command cannot act on the current list and selection. */
  disabled?: ToolbarDisabled;
}

interface Row {
  key: string;
  token: ToolbarToken;
  item?: ToolbarItem;
  /** Position among the buttons, ignoring separators. -1 on a separator. */
  buttonIndex: number;
}

/**
 * The button bar. It draws only the commands in the stored layout, lets a
 * pack repaint it, and keeps the arrow-key walk across the visible buttons
 * even when the reader rearranges them.
 */
export function Toolbar({
  onCommand,
  size,
  layout,
  pack,
  showLabels = true,
  disabled,
}: ToolbarProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const buttonRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const large = size === "large";

  const rows: Row[] = [];
  let buttons = 0;
  layout.forEach((token, position) => {
    if (token === TOOLBAR_SEPARATOR) {
      rows.push({ key: `separator-${position}`, token, buttonIndex: -1 });
      return;
    }
    const item = TOOLBAR_BY_COMMAND.get(token);
    if (!item) {
      return;
    }
    rows.push({ key: item.command, token, item, buttonIndex: buttons });
    buttons += 1;
  });

  // A layout that shrank while the bar was open must not leave the tab stop
  // on a button that is no longer drawn.
  const tabbable = buttons === 0 ? -1 : Math.min(activeIndex, buttons - 1);

  function focusButton(index: number) {
    if (buttons === 0) {
      return;
    }
    const next = (index + buttons) % buttons;
    setActiveIndex(next);
    buttonRefs.current[next]?.focus();
  }

  function handleKeyDown(
    event: React.KeyboardEvent<HTMLButtonElement>,
    index: number,
  ) {
    switch (event.key) {
      case "ArrowRight":
        event.preventDefault();
        focusButton(index + 1);
        break;
      case "ArrowLeft":
        event.preventDefault();
        focusButton(index - 1);
        break;
      case "Home":
        event.preventDefault();
        focusButton(0);
        break;
      case "End":
        event.preventDefault();
        focusButton(buttons - 1);
        break;
    }
  }

  const face = (item: ToolbarItem): ReactNode => (
    <>
      <span className="toolbar-pack-icon grid place-items-center">
        <ToolbarPackIcon
          command={item.command}
          pack={pack}
          className={large ? "size-8" : "size-5"}
        />
      </span>
      {showLabels ? (
        <span
          className={
            "toolbar-pack-label leading-none whitespace-nowrap " +
            (large ? "text-xs" : "text-chrome-micro")
          }
        >
          {item.label}
        </span>
      ) : null}
    </>
  );

  return (
    <div
      role="toolbar"
      aria-label="Downloads toolbar"
      aria-orientation="horizontal"
      data-glow={pack.style.glow ? "true" : undefined}
      style={toolbarPackStyleVars(pack) as CSSProperties}
      className={
        "toolbar-pack flex items-center overflow-x-auto px-1.5 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden " +
        (large ? "h-16" : "h-12")
      }
    >
      {rows.length === 0 ? (
        <button
          type="button"
          onClick={() => onCommand("toolbar-customize")}
          className="toolbar-pack-button rounded-chrome px-2 py-1 text-xs opacity-70 transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:outline-none hover:opacity-100"
        >
          This toolbar has no buttons. Choose the ones to show...
        </button>
      ) : null}

      {rows.map(({ key, item, buttonIndex }) => {
        if (!item) {
          return (
            <span
              key={key}
              role="separator"
              aria-orientation="vertical"
              className={
                "toolbar-pack-separator mx-1 w-px shrink-0 " +
                (large ? "h-9" : "h-7")
              }
            />
          );
        }

        // The item itself knows whether its screen exists; the state knows
        // whether it has anything to act on. Either one can dim it.
        const inert = item.disabled === true || disabled?.[item.command] === true;
        const index = buttonIndex;
        // Without a name under the icon, the button can give back the width
        // the name used to need; the height stays so the bar does not jump.
        const sizing = large
          ? showLabels
            ? "h-15 min-w-18 gap-1 px-2"
            : "h-15 min-w-14 gap-0 px-2"
          : showLabels
            ? "h-11 min-w-14 gap-0.5 px-1.5"
            : "h-11 min-w-11 gap-0 px-1.5";
        const className =
          "toolbar-pack-button flex shrink-0 flex-col items-center justify-center rounded-chrome text-inherit transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
          sizing +
          (inert ? " cursor-default opacity-45" : "");

        return (
          <Tooltip
            key={key}
            content={item.note ? `${item.label} — ${item.note}` : item.label}
            shortcut={item.shortcut}
            side="bottom"
            className="shrink-0"
          >
            <button
              ref={(node) => {
                buttonRefs.current[index] = node;
              }}
              type="button"
              aria-label={item.label}
              aria-disabled={inert || undefined}
              data-inert={inert || undefined}
              tabIndex={tabbable === index ? 0 : -1}
              onClick={() => {
                if (!inert) {
                  onCommand(item.command);
                }
              }}
              onFocus={() => setActiveIndex(index)}
              onKeyDown={(event) => handleKeyDown(event, index)}
              className={className}
            >
              {face(item)}
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}
