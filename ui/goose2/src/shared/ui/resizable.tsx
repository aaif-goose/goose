import type * as React from "react";
import { IconArrowsHorizontal, IconGripVertical } from "@tabler/icons-react";
import * as ResizablePrimitive from "react-resizable-panels";

import { cn } from "@/shared/lib/cn";

function ResizablePanelGroup({
  className,
  ...props
}: React.ComponentProps<typeof ResizablePrimitive.Group>) {
  return (
    <ResizablePrimitive.Group
      data-slot="resizable-panel-group"
      className={cn("flex min-h-0 w-full", className)}
      {...props}
    />
  );
}

function ResizablePanel({
  ...props
}: React.ComponentProps<typeof ResizablePrimitive.Panel>) {
  return <ResizablePrimitive.Panel data-slot="resizable-panel" {...props} />;
}

function ResizableHandle({
  withHandle,
  variant = "default",
  indicator = "grip",
  className,
  handleClassName,
  ...props
}: React.ComponentProps<typeof ResizablePrimitive.Separator> & {
  withHandle?: boolean;
  variant?: "default" | "subtle";
  indicator?: "grip" | "arrows";
  handleClassName?: string;
}) {
  const indicatorIcon =
    indicator === "arrows" ? (
      <IconArrowsHorizontal className="size-2.5" />
    ) : (
      <IconGripVertical className="size-2.5" />
    );

  return (
    <ResizablePrimitive.Separator
      data-slot="resizable-handle"
      className={cn(
        "group focus-visible:ring-ring relative flex w-px shrink-0 items-center justify-center focus-visible:ring-1 focus-visible:ring-offset-1 focus-visible:outline-hidden after:absolute after:inset-y-0 after:left-1/2 after:-translate-x-1/2 aria-[orientation=horizontal]:h-px aria-[orientation=horizontal]:w-full aria-[orientation=horizontal]:after:left-0 aria-[orientation=horizontal]:after:top-1/2 aria-[orientation=horizontal]:after:-translate-y-1/2 aria-[orientation=horizontal]:after:translate-x-0 [&[aria-orientation=horizontal]>div]:rotate-90",
        variant === "subtle"
          ? "bg-transparent transition-colors after:w-5 hover:bg-border data-[separator=active]:bg-border-default focus-visible:bg-border-default aria-[orientation=horizontal]:after:h-5 aria-[orientation=horizontal]:after:w-full"
          : "bg-border-default after:w-1 aria-[orientation=horizontal]:after:h-1 aria-[orientation=horizontal]:after:w-full",
        className,
      )}
      {...props}
    >
      {withHandle && (
        <div
          className={cn(
            "z-10 flex items-center justify-center border text-muted-foreground",
            variant === "subtle"
              ? "bg-background/95 shadow-xs h-5 w-5 rounded-full border-border-soft opacity-0 transition-all duration-150 ease-out group-hover:scale-100 group-hover:opacity-100 group-focus-visible:scale-100 group-focus-visible:opacity-100 group-data-[separator=active]:scale-100 group-data-[separator=active]:opacity-100 scale-95"
              : "bg-border-default h-4 w-3 rounded-xs",
            handleClassName,
          )}
        >
          {indicatorIcon}
        </div>
      )}
    </ResizablePrimitive.Separator>
  );
}

export { ResizablePanelGroup, ResizablePanel, ResizableHandle };
