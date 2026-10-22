import { useRef, useState } from "react";

import type { ToolbarSize } from "@/lib/settings";

import type { ShellCommand } from "./commands";
import { TOOLBAR_ITEMS } from "./toolbar-items";

interface ToolbarProps {
  onCommand: (command: ShellCommand) => void;
  /** Large buttons are the default; small ones make room for the list. */
  size: ToolbarSize;
}

export function Toolbar({ onCommand, size }: ToolbarProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const buttonRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const large = size === "large";

  function focusButton(index: number) {
    const next = (index + TOOLBAR_ITEMS.length) % TOOLBAR_ITEMS.length;
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
        focusButton(TOOLBAR_ITEMS.length - 1);
        break;
    }
  }

  return (
    <div
      role="toolbar"
      aria-label="Downloads toolbar"
      aria-orientation="horizontal"
      className={
        "flex items-center overflow-x-auto border-b border-chrome-canvas bg-chrome-toolbar px-1.5 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden " +
        (large ? "h-16" : "h-12")
      }
    >
      {TOOLBAR_ITEMS.map(({ command, label, icon: Icon, disabled }, index) => (
        <button
          key={command}
          ref={(node) => {
            buttonRefs.current[index] = node;
          }}
          type="button"
          title={label}
          aria-disabled={disabled || undefined}
          tabIndex={activeIndex === index ? 0 : -1}
          onClick={() => {
            if (!disabled) {
              onCommand(command);
            }
          }}
          onFocus={() => setActiveIndex(index)}
          onKeyDown={(event) => handleKeyDown(event, index)}
          className={
            "flex shrink-0 flex-col items-center justify-center rounded-chrome border border-transparent text-chrome-toolbar-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
            (large
              ? "h-15 min-w-18 gap-1 px-2"
              : "h-11 min-w-14 gap-0.5 px-1.5") +
            (disabled
              ? " cursor-default opacity-45"
              : " hover:border-chrome-line hover:bg-chrome-toolbar-hover active:bg-chrome-toolbar-active")
          }
        >
          <Icon className={large ? "size-8" : "size-5"} />
          <span
            className={
              large ? "text-xs leading-none whitespace-nowrap" : "text-chrome-micro leading-none whitespace-nowrap"
            }
          >
            {label}
          </span>
        </button>
      ))}
    </div>
  );
}
