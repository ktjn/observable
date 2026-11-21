import { forwardRef } from "react";
import type { ButtonHTMLAttributes } from "react";
import { Button as DesignedButton } from "@ktjn/designed-react";
import { cn } from "./cn";

type ButtonVariant = "primary" | "secondary" | "destructive" | "ghost";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
}

const designedVariant: Record<ButtonVariant, "primary" | "secondary" | "danger" | "ghost"> = {
  primary: "primary",
  secondary: "secondary",
  destructive: "danger",
  ghost: "ghost",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, type = "button", variant = "primary", ...props },
  ref
) {
  return (
    <DesignedButton
      ref={ref}
      type={type}
      variant={designedVariant[variant]}
      className={cn("text-[11px]", className)}
      {...props}
    />
  );
});
