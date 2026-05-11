import type { ReactNode } from "react";
import { cn } from "@/shared/lib/cn";

interface SettingsPageProps {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  controls?: ReactNode;
  children?: ReactNode;
  className?: string;
  contentClassName?: string;
}

export function SettingsPage({
  title,
  description,
  actions,
  controls,
  children,
  className,
  contentClassName,
}: SettingsPageProps) {
  return (
    <div className={cn("min-h-full", className)}>
      <div className="sticky top-0 z-20 -mx-6 border-b bg-background px-6 py-4">
        <div className="flex items-center justify-between gap-3 pr-12">
          <div className="min-w-0 flex-1">
            <h3 className="max-w-prose truncate font-display text-sm font-semibold leading-5 tracking-tight">
              {title}
            </h3>
            {description ? (
              <p className="mt-0.5 max-w-prose text-xs leading-4 text-muted-foreground">
                {description}
              </p>
            ) : null}
          </div>
          {actions ? (
            <div className="flex flex-shrink-0 items-center gap-1.5">
              {actions}
            </div>
          ) : null}
        </div>
        {controls ? <div className="mt-3 pr-12">{controls}</div> : null}
      </div>
      {children ? (
        <div className={cn("py-3", contentClassName)}>{children}</div>
      ) : null}
    </div>
  );
}
