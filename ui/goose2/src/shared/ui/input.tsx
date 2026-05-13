import type * as React from "react";

import { cn } from "@/shared/lib/cn";

const variantStyles = {
  default: [
    "file:text-foreground placeholder:text-placeholder selection:bg-primary selection:text-primary-foreground border-input hover:border-border-input-hover focus-visible:border-ring focus-visible:ring-0 focus-visible:ring-offset-0 flex h-9 w-full min-w-0 rounded-input border bg-transparent px-3 py-1 text-base transition-[color,box-shadow] outline-none file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 md:text-sm",
    "aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
  ],
  ghost: [
    "w-full border-none bg-transparent text-sm text-foreground shadow-none outline-none placeholder:text-muted-foreground focus:outline-none focus:ring-0 disabled:cursor-not-allowed disabled:opacity-50",
  ],
} as const;

interface InputProps extends React.ComponentProps<"input"> {
  variant?: keyof typeof variantStyles;
  inputRef?: React.Ref<HTMLInputElement>;
}

function Input({
  className,
  type,
  variant = "default",
  inputRef,
  ...props
}: InputProps) {
  return (
    <input
      ref={inputRef}
      type={type}
      data-slot="input"
      className={cn(...variantStyles[variant], className)}
      {...props}
    />
  );
}

export { Input };
