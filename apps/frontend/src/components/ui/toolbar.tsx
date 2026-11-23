import type { HTMLAttributes, ReactNode } from "react";
import { Toolbar as DesignedToolbar } from "@ktjn/designed-react";
import { cn } from "./cn";

export interface ToolbarProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export function Toolbar({ className, children, ...props }: ToolbarProps) {
  return (
    <DesignedToolbar
      className={cn("modern-toolbar rounded-none border-0 bg-transparent p-0", className)}
      {...props}
    >
      {children}
    </DesignedToolbar>
  );
}
