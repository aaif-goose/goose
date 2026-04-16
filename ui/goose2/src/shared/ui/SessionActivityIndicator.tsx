import { Loader2 } from "lucide-react";
import { cn } from "@/shared/lib/cn";

interface SessionActivityIndicatorProps {
  isRunning?: boolean;
  hasUnread?: boolean;
  variant?: "inline" | "overlay";
  className?: string;
}

export function SessionActivityIndicator({
  isRunning = false,
  hasUnread = false,
  variant = "inline",
  className,
}: SessionActivityIndicatorProps) {
  if (isRunning) {
    if (variant === "overlay") {
      return (
        <span
          role="status"
          aria-label="Chat active"
          className={cn(
            "absolute -right-1 -top-1 flex h-3.5 w-3.5 items-center justify-center rounded-full border border-background bg-background shadow-sm",
            className,
          )}
        >
          <Loader2
            aria-hidden="true"
            className="h-2.5 w-2.5 animate-spin text-brand"
          />
        </span>
      );
    }

    return (
      <Loader2
        role="status"
        aria-label="Chat active"
        className={cn("h-3 w-3 shrink-0 animate-spin text-brand", className)}
      />
    );
  }

  if (!hasUnread) {
    return null;
  }

  if (variant === "overlay") {
    return (
      <span
        role="status"
        aria-label="Unread messages"
        className={cn(
          "absolute -right-0.5 -top-0.5 h-2.5 w-2.5 shrink-0 rounded-full border border-background bg-brand",
          className,
        )}
      />
    );
  }

  return (
    <span
      role="status"
      aria-label="Unread messages"
      className={cn("h-2 w-2 shrink-0 rounded-full bg-brand", className)}
    />
  );
}
