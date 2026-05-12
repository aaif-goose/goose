import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "@/shared/lib/cn";

export function PickerItem({
  children,
  onClick,
  selected = false,
  disabled = false,
  className,
  ...rest
}: {
  children: ReactNode;
  selected?: boolean;
} & Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type">) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "flex min-w-0 w-full items-center gap-2 overflow-hidden rounded-sm px-2 py-1.5 text-left text-sm transition-colors",
        "hover:bg-muted focus-visible:bg-muted focus:outline-none",
        "disabled:pointer-events-none disabled:opacity-50",
        selected && "bg-muted/60",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}
