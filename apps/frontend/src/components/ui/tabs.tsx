import { Tabs as BaseTabs } from "@base-ui/react/tabs";
import { forwardRef } from "react";
import type { ComponentPropsWithoutRef, ElementRef } from "react";
import { cn } from "./cn";

function mergeClassName<State>(
  baseClassName: string,
  className?: string | ((state: State) => string | undefined)
) {
  if (typeof className === "function") {
    return (state: State) => cn(baseClassName, className(state));
  }

  return cn(baseClassName, className);
}

export function TabsRoot(props: BaseTabs.Root.Props) {
  return <BaseTabs.Root {...props} />;
}

export const TabsList = forwardRef<
  ElementRef<typeof BaseTabs.List>,
  ComponentPropsWithoutRef<typeof BaseTabs.List>
>(function TabsList({ className, ...props }, ref) {
  return (
    <BaseTabs.List
      ref={ref}
      className={mergeClassName(
        "relative inline-flex min-h-8 items-end gap-0 border-b border-[var(--border-strong)] bg-transparent",
        className
      )}
      {...props}
    />
  );
});

export const TabsTab = forwardRef<
  ElementRef<typeof BaseTabs.Tab>,
  ComponentPropsWithoutRef<typeof BaseTabs.Tab>
>(function TabsTab({ className, ...props }, ref) {
  return (
    <BaseTabs.Tab
      ref={ref}
      className={mergeClassName(
        cn(
          "inline-flex min-h-7 items-center justify-center border-b-2 border-transparent px-3 text-[11px] font-medium text-[var(--muted)] outline-none transition-colors",
          "data-[selected]:border-[var(--brand)] data-[selected]:text-[var(--text)] data-[selected]:font-semibold",
          "hover:text-[var(--text)]",
          "focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-1 focus-visible:ring-offset-[var(--bg)]"
        ),
        className
      )}
      {...props}
    />
  );
});

export const TabsPanel = forwardRef<
  ElementRef<typeof BaseTabs.Panel>,
  ComponentPropsWithoutRef<typeof BaseTabs.Panel>
>(function TabsPanel({ className, ...props }, ref) {
  return (
    <BaseTabs.Panel
      ref={ref}
      className={mergeClassName("outline-none", className)}
      {...props}
    />
  );
});

export const Tabs = {
  Root: TabsRoot,
  List: TabsList,
  Tab: TabsTab,
  Panel: TabsPanel,
};
