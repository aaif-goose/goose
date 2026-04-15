import type { ReactNode } from "react";

interface WidgetProps {
  title: ReactNode;
  icon: ReactNode;
  action?: ReactNode;
  flush?: boolean;
  children: ReactNode;
}

export function Widget({ title, icon, action, flush, children }: WidgetProps) {
  return (
    <div className="overflow-hidden rounded-md border border-border">
      <div className="flex h-8 items-center justify-between bg-background-alt px-3">
        <div className="flex min-w-0 items-center gap-2 text-xs font-medium text-foreground">
          {icon}
          {title}
        </div>
        {action}
      </div>
      {flush ? (
        children
      ) : (
        <div className="px-3 py-2.5 text-xs text-foreground-subtle">
          {children}
        </div>
      )}
    </div>
  );
}
