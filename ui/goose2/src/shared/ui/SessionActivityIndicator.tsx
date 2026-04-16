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
            "absolute -right-1 -top-1 flex h-3.5 w-3.5 items-center justify-center rounded-full border border-background bg-background shadow-sm transition-opacity duration-200 ease-out animate-in fade-in-0",
            className,
          )}
        >
          <Loader2
            aria-hidden="true"
            className="h-2.5 w-2.5 animate-spin text-text-info"
          />
        </span>
      );
    }

    return (
      <span
        role="status"
        aria-label="Chat active"
        className={cn(
          "inline-flex h-3 w-3 shrink-0 items-center justify-center animate-in fade-in-0 duration-200 ease-out",
          className,
        )}
      >
        <Loader2
          aria-hidden="true"
          className="h-3 w-3 animate-spin text-text-info"
        />
      </span>
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
          "absolute -right-0.5 -top-0.5 h-2 w-2 shrink-0 rounded-full border border-background bg-background-info transition-opacity duration-200 ease-out animate-in fade-in-0",
          className,
        )}
      />
    );
  }

  return (
    <span
      role="status"
      aria-label="Unread messages"
      className={cn(
        "h-1.5 w-1.5 shrink-0 rounded-full bg-background-info transition-opacity duration-200 ease-out animate-in fade-in-0",
        className,
      )}
    />
  );
}
