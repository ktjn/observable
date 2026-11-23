import { Tabs as DesignedTabs } from "@ktjn/designed-react";
import { forwardRef } from "react";
import type { ComponentPropsWithoutRef, ElementRef } from "react";
import { cn } from "./cn";

export function TabsRoot(props: ComponentPropsWithoutRef<typeof DesignedTabs>) {
  return <DesignedTabs {...props} />;
}

export const TabsList = forwardRef<
  ElementRef<typeof DesignedTabs.List>,
  ComponentPropsWithoutRef<typeof DesignedTabs.List>
>(function TabsList({ className, ...props }, ref) {
  return (
    <DesignedTabs.List
      ref={ref}
      className={cn(
        "relative inline-flex min-h-8 items-end gap-0 border-b border-[var(--border-strong)] bg-transparent",
        className
      )}
      {...props}
    />
  );
});

export const TabsTab = forwardRef<
  ElementRef<typeof DesignedTabs.Tab>,
  ComponentPropsWithoutRef<typeof DesignedTabs.Tab>
>(function TabsTab({ className, ...props }, ref) {
  return (
    <DesignedTabs.Tab
      ref={ref}
      size="small"
      className={cn(
        "inline-flex min-h-7 items-center justify-center border-b-2 border-transparent px-3 text-[11px] font-medium text-[var(--muted)] outline-none transition-colors",
        "data-[selected]:border-[var(--brand)] data-[selected]:text-[var(--text)] data-[selected]:font-semibold",
        "hover:text-[var(--text)]",
        "focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-1 focus-visible:ring-offset-[var(--bg)]",
        className
      )}
      {...props}
    />
  );
});

export const TabsPanel = forwardRef<
  ElementRef<typeof DesignedTabs.Panel>,
  ComponentPropsWithoutRef<typeof DesignedTabs.Panel>
>(function TabsPanel({ className, ...props }, ref) {
  return (
    <DesignedTabs.Panel
      ref={ref}
      className={cn("outline-none", className)}
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
