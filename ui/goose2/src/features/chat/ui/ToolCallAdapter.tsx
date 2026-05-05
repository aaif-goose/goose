import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { FolderOpen, ChevronRight } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  Tool,
  ToolHeader,
  ToolContent,
  ToolInput,
  ToolOutput,
} from "@/shared/ui/ai-elements/tool";
import { toolStatusMap } from "../lib/toolStatusMap";
import type { ToolCallLocation, ToolCallStatus } from "@/shared/types/messages";
import { useArtifactPolicyContext } from "@/features/chat/hooks/ArtifactPolicyContext";

interface ToolCallAdapterProps {
  name: string;
  arguments: Record<string, unknown>;
  status: ToolCallStatus;
  locations?: ToolCallLocation[];
  result?: string;
  structuredContent?: unknown;
  isError?: boolean;
  /** Epoch ms when the tool call started executing. */
  startedAt?: number;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

function useElapsedTime(status: ToolCallStatus, startedAt?: number) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (status === "executing") {
      const origin = startedAt ?? Date.now();
      // Compute initial elapsed immediately so the first render is accurate.
      setElapsed(Math.floor((Date.now() - origin) / 1000));
      const interval = setInterval(() => {
        setElapsed(Math.floor((Date.now() - origin) / 1000));
      }, 1000);
      return () => clearInterval(interval);
    }
    setElapsed(0);
  }, [status, startedAt]);

  return elapsed;
}

function getLocationKind(path: string): "file" | "folder" | "path" {
  const normalized = path.trim();
  if (normalized.endsWith("/") || normalized.endsWith("\\")) return "folder";
  const name =
    normalized
      .split(/[\\/]+/)
      .filter(Boolean)
      .pop() ?? normalized;
  const dot = name.lastIndexOf(".");
  return dot > 0 && dot < name.length - 1 ? "file" : "path";
}

function visibleLocations(locations: ToolCallLocation[] | undefined) {
  const seen = new Set<string>();
  return (locations ?? []).filter(
    (location): location is ToolCallLocation & { path: string } => {
      if (
        typeof location.path !== "string" ||
        location.path.trim().length === 0
      ) {
        return false;
      }
      const key = `${location.path}:${location.line ?? ""}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    },
  );
}

function ArtifactActions({ locations }: { locations?: ToolCallLocation[] }) {
  const { t } = useTranslation(["chat", "common"]);
  const [moreOutputsOpen, setMoreOutputsOpen] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);
  const { openResolvedPath } = useArtifactPolicyContext();
  const artifactLocations = visibleLocations(locations);

  if (artifactLocations.length === 0) return null;

  const openLocation = async (location: ToolCallLocation) => {
    try {
      setOpenError(null);
      await openResolvedPath(location.path);
    } catch (error) {
      setOpenError(error instanceof Error ? error.message : String(error));
    }
  };

  const primary = artifactLocations[0];
  const secondaryLocations = artifactLocations.slice(1);
  const kindLabel: Record<string, string> = {
    file: t("tools.openFile"),
    folder: t("tools.openFolder"),
    path: t("tools.openPath"),
  };

  const renderLocationButton = (
    location: ToolCallLocation & { path: string },
    className: string,
    iconClassName: string,
  ) => {
    const kind = getLocationKind(location.path);
    return (
      <Button
        type="button"
        variant="outline-flat"
        onClick={() => void openLocation(location)}
        className={className}
        title={location.path}
      >
        <FolderOpen className={iconClassName} />
        <span className="truncate">
          {kindLabel[kind] ?? t("common:actions.open")}
        </span>
        <span className="truncate text-[10px] text-muted-foreground">
          {location.path}
        </span>
      </Button>
    );
  };

  return (
    <div className="mt-1.5 ml-1 space-y-1.5">
      {renderLocationButton(
        primary,
        "h-auto max-w-full justify-start rounded-md border-accent/45 bg-background px-2.5 py-1 text-xs text-accent-foreground hover:bg-accent/55",
        "h-3.5 w-3.5 shrink-0",
      )}

      {secondaryLocations.length > 0 && (
        <div className="space-y-1">
          <button
            type="button"
            onClick={() => setMoreOutputsOpen((prev) => !prev)}
            className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
          >
            <ChevronRight
              className={cn(
                "h-3 w-3 transition-transform",
                moreOutputsOpen && "rotate-90",
              )}
            />
            {t("tools.moreOutputs", {
              count: secondaryLocations.length,
            })}
          </button>
          {moreOutputsOpen && (
            <div className="space-y-1.5 pl-4">
              {secondaryLocations.map((location) => (
                <div
                  key={`${location.path}:${location.line ?? ""}`}
                  className="space-y-0.5"
                >
                  {renderLocationButton(
                    location,
                    "h-auto max-w-full justify-start rounded-md border-border bg-background px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground",
                    "h-3 w-3 shrink-0",
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {openError && <p className="text-[11px] text-destructive">{openError}</p>}
    </div>
  );
}

export function ToolCallAdapter({
  name,
  arguments: args,
  status,
  locations,
  result,
  structuredContent,
  isError,
  startedAt,
  open,
  onOpenChange,
}: ToolCallAdapterProps) {
  const { t } = useTranslation("chat");
  const elapsed = useElapsedTime(status, startedAt);
  const state = toolStatusMap[status];

  const elapsedSeconds =
    status === "executing" && elapsed >= 3 ? elapsed : undefined;
  const outputViewportClassName = cn(
    "max-h-[28rem] overflow-auto",
    "[scrollbar-color:hsl(var(--muted-foreground))_transparent] [scrollbar-width:thin]",
    "[&::-webkit-scrollbar]:h-2 [&::-webkit-scrollbar]:w-2 [&::-webkit-scrollbar-track]:bg-transparent",
    "[&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-muted-foreground/50",
  );

  return (
    <div className="w-full min-w-0 max-w-full">
      <Tool open={open} onOpenChange={onOpenChange}>
        <ToolHeader
          type="dynamic-tool"
          toolName={name}
          title={name}
          state={state}
          showIcon={false}
          elapsedSeconds={elapsedSeconds}
        />
        <ToolContent>
          {Object.keys(args).length > 0 && <ToolInput input={args} />}
          <ToolOutput
            output={isError ? undefined : result}
            errorText={isError ? result : undefined}
            label={isError ? undefined : t("tools.content")}
            contentClassName={outputViewportClassName}
            plainText
          />
          {!isError && structuredContent !== undefined && (
            <ToolOutput
              output={structuredContent}
              errorText={undefined}
              label={t("tools.structuredContent")}
              contentClassName={outputViewportClassName}
            />
          )}
        </ToolContent>
      </Tool>
      <ArtifactActions locations={locations} />
    </div>
  );
}
