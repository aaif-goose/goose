import type { ReactNode } from "react";
import { cn } from "@/shared/lib/cn";

/**
 * Layout wrapper for full-page views (Skills, Agents, etc.).
 *
 * Fills the parent container (typically `<main>` inside AppShell)
 * and provides a flex-column context for header + scrollable content.
 */
export function MainPanelLayout({
  children,
  backgroundColor = "bg-background",
  className,
}: {
  children: ReactNode;
  backgroundColor?: string;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex h-full min-h-0 min-w-0 flex-col",
        backgroundColor,
        className,
      )}
    >
      {children}
    </div>
  );
}
