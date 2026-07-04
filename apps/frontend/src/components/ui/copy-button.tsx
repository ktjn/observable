import { useEffect, useRef, useState } from "react";
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from "react";
import { Check, Copy } from "lucide-react";
import { cn } from "./cn";

type CopyState = "idle" | "copied" | "error";

async function copyToClipboard(value: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value);
    return;
  }
  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  try {
    const ok = document.execCommand("copy");
    if (!ok) throw new Error("execCommand('copy') returned false");
  } finally {
    document.body.removeChild(textarea);
  }
}

export function useCopyToClipboard(resetMs = 1500): {
  state: CopyState;
  copy: (value: string) => Promise<void>;
} {
  const [state, setState] = useState<CopyState>("idle");
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    return () => clearTimeout(timeoutRef.current);
  }, []);

  const copy = async (value: string) => {
    clearTimeout(timeoutRef.current);
    try {
      await copyToClipboard(value);
      setState("copied");
    } catch (err) {
      console.error("copy failed", err);
      setState("error");
    }
    timeoutRef.current = setTimeout(() => setState("idle"), resetMs);
  };

  return { state, copy };
}

const sizeClasses = {
  xs: "size-3",
  sm: "size-3.5",
} as const;

export interface CopyButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onClick" | "children"> {
  value: string;
  label?: string;
  size?: "xs" | "sm";
  visibility?: "hover" | "always";
}

export function CopyButton({
  value,
  label = "Copy",
  size = "xs",
  visibility = "hover",
  className,
  ...props
}: CopyButtonProps) {
  const { state, copy } = useCopyToClipboard();
  const copied = state === "copied";

  return (
    <button
      type="button"
      aria-label={copied ? "Copied" : label}
      title={label}
      onClick={(e) => {
        e.stopPropagation();
        e.preventDefault();
        void copy(value);
        // A mouse click leaves the button natively :focus'd (though not
        // :focus-visible), which keeps an ancestor's :focus-within true and
        // the hover-only button visible indefinitely. Blur so it returns to
        // hover-only visibility once the click is handled; keyboard users
        // still get :focus-visible when tabbing to it.
        e.currentTarget.blur();
      }}
      className={cn(
        "inline-flex shrink-0 items-center justify-center text-[var(--muted)] outline-none transition-opacity hover:text-[var(--brand)]",
        "focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)]",
        visibility === "hover" &&
          cn(
            "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100",
            copied && "opacity-100"
          ),
        className
      )}
      {...props}
    >
      {copied ? (
        <Check className={sizeClasses[size]} />
      ) : (
        <Copy className={sizeClasses[size]} />
      )}
    </button>
  );
}

export interface CopyableTextProps extends HTMLAttributes<HTMLSpanElement> {
  value: string;
  children?: ReactNode;
  label?: string;
  size?: "xs" | "sm";
  mono?: boolean;
}

export function CopyableText({
  value,
  children,
  label,
  size = "xs",
  mono,
  className,
  ...props
}: CopyableTextProps) {
  return (
    <span
      className={cn("group inline-flex min-w-0 max-w-full items-center gap-1", className)}
      {...props}
    >
      <span className={cn("truncate", mono && "font-mono")}>{children ?? value}</span>
      <CopyButton value={value} label={label} size={size} />
    </span>
  );
}
