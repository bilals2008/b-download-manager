import { useRef, useState } from "react";

import type { ShellCommand } from "./commands";
import { TOOLBAR_ITEMS } from "./toolbar-items";

interface ToolbarProps {
  onCommand: (command: ShellCommand) => void;
}

export function Toolbar({ onCommand }: ToolbarProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const buttonRefs = useRef<Array<HTMLButtonElement | null>>([]);

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
      className="flex h-16 items-center overflow-x-auto border-b border-chrome-canvas bg-chrome-toolbar px-1.5 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
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
            "flex h-15 min-w-18 shrink-0 flex-col items-center justify-center gap-1 rounded-chrome border border-transparent px-2 text-chrome-toolbar-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
            (disabled
              ? "cursor-default opacity-45"
              : "hover:border-chrome-line hover:bg-chrome-toolbar-hover active:bg-chrome-toolbar-active")
          }
        >
          <Icon className="size-8" />
          <span className="text-xs leading-none whitespace-nowrap">{label}</span>
        </button>
      ))}
    </div>
  );
}
