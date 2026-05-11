import type { ReactNode } from "react";
import { cn } from "@/shared/lib/cn";

interface DetailFieldProps {
  label: ReactNode;
  children?: ReactNode;
  meta?: ReactNode;
  className?: string;
  headerClassName?: string;
  labelClassName?: string;
  contentClassName?: string;
  contentAs?: "div" | "p";
}

export function DetailField({
  label,
  children,
  meta,
  className,
  headerClassName,
  labelClassName,
  contentClassName,
  contentAs = "div",
}: DetailFieldProps) {
  const Content = contentAs;

  return (
    <div className={cn("space-y-2", className)}>
      <div
        className={cn(
          "flex items-center justify-between gap-3",
          headerClassName,
        )}
      >
        <p
          className={cn(
            "text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground",
            labelClassName,
          )}
        >
          {label}
        </p>
        {meta}
      </div>
      {children !== undefined && children !== null ? (
        <Content className={cn("text-sm text-foreground", contentClassName)}>
          {children}
        </Content>
      ) : null}
    </div>
  );
}
