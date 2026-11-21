import { forwardRef } from "react";
import type { InputHTMLAttributes } from "react";
import { Input as DesignedInput } from "@ktjn/designed-react";
import { cn } from "./cn";

export type InputProps = InputHTMLAttributes<HTMLInputElement>;

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className, type = "text", ...props },
  ref
) {
  return (
    <DesignedInput
      ref={ref}
      type={type}
      className={cn("font-[family-name:'IBM_Plex_Mono',monospace] text-[11px]", className)}
      {...props}
    />
  );
});
