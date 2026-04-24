import type { ReactNode } from "react";

/**
 * Layout wrapper for full-page views (Skills, Agents, etc.).
 *
 * Fills the parent container (typically `<main>` inside AppShell)
 * and provides a flex-column context for header + scrollable content.
 */
export function MainPanelLayout({
  children,
  backgroundColor = "bg-background",
}: {
  children: ReactNode;
  backgroundColor?: string;
}) {
  return (
    <div className={`flex flex-col ${backgroundColor} min-w-0 h-full min-h-0`}>
      {children}
    </div>
  );
}
