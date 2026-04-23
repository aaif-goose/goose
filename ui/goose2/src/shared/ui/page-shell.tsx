import type { ReactNode } from "react";
import { cn } from "@/shared/lib/cn";
import { MainPanelLayout } from "./MainPanelLayout";

interface ShellProps {
  children: ReactNode;
  className?: string;
  contentClassName?: string;
}

interface PageHeaderProps {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  titleElement?: "h1" | "div";
  className?: string;
  titleClassName?: string;
  descriptionClassName?: string;
  actionsClassName?: string;
}

export function PageShell({
  children,
  className,
  contentClassName,
}: ShellProps) {
  return (
    <MainPanelLayout className={className}>
      <div className="min-h-0 flex-1 overflow-y-scroll [scrollbar-gutter:stable]">
        <div
          className={cn(
            "mx-auto flex w-full max-w-5xl flex-col gap-8 px-6 py-8 page-transition",
            contentClassName,
          )}
        >
          {children}
        </div>
      </div>
    </MainPanelLayout>
  );
}

export function DetailPageShell({
  children,
  className,
  contentClassName,
}: ShellProps) {
  return (
    <MainPanelLayout className={className}>
      <div className="min-h-0 flex-1 overflow-y-scroll [scrollbar-gutter:stable]">
        <div
          className={cn(
            "mx-auto flex w-full max-w-5xl flex-col gap-8 px-6 py-8 page-transition",
            contentClassName,
          )}
        >
          {children}
        </div>
      </div>
    </MainPanelLayout>
  );
}

export function PageHeader({
  title,
  description,
  actions,
  titleElement = "h1",
  className,
  titleClassName,
  descriptionClassName,
  actionsClassName,
}: PageHeaderProps) {
  const TitleElement = titleElement;

  return (
    <div
      className={cn(
        "flex flex-wrap items-start justify-between gap-4",
        className,
      )}
    >
      <div>
        <TitleElement className={cn("text-xl tracking-tight", titleClassName)}>
          {title}
        </TitleElement>
        {description ? (
          <p
            className={cn(
              "mt-1 text-sm font-light text-muted-foreground",
              descriptionClassName,
            )}
          >
            {description}
          </p>
        ) : null}
      </div>
      {actions ? (
        <div className={cn("flex items-start gap-2", actionsClassName)}>
          {actions}
        </div>
      ) : null}
    </div>
  );
}

export function FilterRow({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-wrap items-center gap-2", className)}>
      {children}
    </div>
  );
}
