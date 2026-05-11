import { useId, type CSSProperties, type ReactNode } from "react";
import { useIsMobile } from "@/hooks/use-mobile";
import { cn } from "@/shared/lib/cn";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/shared/ui/resizable";

interface PageColumnsProps {
  sidebar: ReactNode;
  children: ReactNode;
  className?: string;
  sidebarClassName?: string;
  contentClassName?: string;
  resizable?: boolean;
  defaultSidebarSize?: number;
  minSidebarSize?: number;
  maxSidebarSize?: number;
  minContentSize?: number;
}

export function PageColumns({
  sidebar,
  children,
  className,
  sidebarClassName,
  contentClassName,
  resizable = true,
  defaultSidebarSize = 28,
  minSidebarSize = 22,
  maxSidebarSize = 38,
  minContentSize = 48,
}: PageColumnsProps) {
  const isMobile = useIsMobile();
  const pageColumnsId = useId();
  const sidebarSize = `${defaultSidebarSize}%`;
  const sidebarMinSize = `${minSidebarSize}%`;
  const sidebarMaxSize = `${maxSidebarSize}%`;
  const contentSize = `${100 - defaultSidebarSize}%`;
  const contentMinSize = `${minContentSize}%`;
  const stackedLayoutStyle = {
    "--page-columns-sidebar-size": sidebarSize,
  } as CSSProperties;
  const panelContentStyle = {
    overflow: "visible",
    maxHeight: "none",
    flexGrow: 0,
  } as CSSProperties;

  if (isMobile || !resizable) {
    return (
      <div
        className={cn(
          "grid gap-10 lg:grid-cols-[minmax(0,var(--page-columns-sidebar-size))_minmax(0,1fr)]",
          className,
        )}
        style={stackedLayoutStyle}
      >
        <div className={cn("min-w-0", sidebarClassName)}>{sidebar}</div>
        <div className={cn("min-w-0", contentClassName)}>{children}</div>
      </div>
    );
  }

  return (
    <ResizablePanelGroup
      orientation="horizontal"
      resizeTargetMinimumSize={{ coarse: 36, fine: 12 }}
      className={cn("min-h-0 min-w-0 items-stretch", className)}
      style={{ height: "auto", overflow: "visible" }}
    >
      <ResizablePanel
        id={`${pageColumnsId}-sidebar`}
        defaultSize={sidebarSize}
        minSize={sidebarMinSize}
        maxSize={sidebarMaxSize}
        className="min-w-0 overflow-visible"
        style={panelContentStyle}
      >
        <div className={cn("min-w-0 pr-5", sidebarClassName)}>{sidebar}</div>
      </ResizablePanel>
      <ResizableHandle
        withHandle
        variant="subtle"
        indicator="arrows"
        className="data-[separator=active]:bg-border-default hover:bg-border"
      />
      <ResizablePanel
        id={`${pageColumnsId}-content`}
        defaultSize={contentSize}
        minSize={contentMinSize}
        className="min-w-0 overflow-visible"
        style={panelContentStyle}
      >
        <div className={cn("min-w-0 pl-5", contentClassName)}>{children}</div>
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
