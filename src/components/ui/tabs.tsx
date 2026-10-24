import * as React from "react";

import { cn } from "@/lib/utils";

const TABS_TRIGGER =
  "inline-flex h-8 shrink-0 items-center justify-center rounded-chrome px-2.5 text-xs font-medium text-chrome-menu-text transition-colors hover:bg-chrome-menu-hover hover:text-chrome-title-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none data-[state=active]:bg-chrome-menu-active data-[state=active]:text-chrome-title-text disabled:cursor-default disabled:text-chrome-muted/50 disabled:hover:bg-transparent disabled:hover:text-chrome-muted/50";

interface TabsProps {
  value: string;
  onValueChange: (value: string) => void;
  children: React.ReactNode;
  className?: string;
}

interface TabsListProps {
  children: React.ReactNode;
  className?: string;
}

interface TabsTriggerProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  value: string;
  disabled?: boolean;
  className?: string;
  children: React.ReactNode;
}

interface TabsContentProps {
  value: string;
  className?: string;
  children: React.ReactNode;
}

type TabsContextValue = {
  value: string;
  onValueChange: (value: string) => void;
} | null;

const TabsContext = React.createContext<TabsContextValue>(null);

/**
 * The tab bar at the top of the settings window. It wraps instead of
 * scrolling so every page stays visible in one glance.
 */
export function TabsList({
  children,
  className,
}: TabsListProps & { children: React.ReactNode }) {
  return (
    <div
      className={cn(
        "flex w-full shrink-0 flex-wrap items-center gap-1 border-b border-chrome-line bg-chrome-panel px-2.5 py-1",
        className,
      )}
    >
      {children}
    </div>
  );
}

/** One tab: an icon or label that switches the settings page. */
export function TabsTrigger({
  value,
  disabled,
  children,
  className,
  ...props
}: TabsTriggerProps & { children: React.ReactNode }) {
  const context = React.useContext(TabsContext);
  if (!context) {
    throw new Error("TabsTrigger must be used within TabsList inside Tabs.");
  }

  const { value: activeValue, onValueChange } = context;
  const active = activeValue === value;

  return (
    <button
      type="button"
      data-state={active ? "active" : "inactive"}
      data-disabled={disabled ? "true" : undefined}
      disabled={disabled}
      onClick={() => !disabled && onValueChange(value)}
      className={cn(TABS_TRIGGER, className)}
      {...props}
    >
      {children}
    </button>
  );
}

/** The panel under one tab: hidden unless its tab is active. */
export function TabsContent({
  value,
  className,
  children,
}: TabsContentProps & { children: React.ReactNode }) {
  const context = React.useContext(TabsContext);
  const active = context ? context.value === value : false;

  if (!active) {
    return null;
  }

  return (
    <div
      className={cn(
        "min-h-0 flex-1 overflow-y-auto scrollbar-thin motion-safe:animate-page-in motion-reduce:animate-none",
        className,
      )}
    >
      {children}
    </div>
  );
}

/** The settings tab group: a bar of triggers above the pages. */
export function Tabs({
  value,
  onValueChange,
  children,
  className,
}: TabsProps & { children: React.ReactNode }) {
  return (
    <TabsContext.Provider value={{ value, onValueChange }}>
      <div className={cn("flex w-full flex-col gap-0", className)}>
        {children}
      </div>
    </TabsContext.Provider>
  );
}
