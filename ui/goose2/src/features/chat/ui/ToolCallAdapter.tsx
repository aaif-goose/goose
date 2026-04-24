import { useState, useEffect, useMemo } from "react";
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
import type { ToolCallStatus } from "@/shared/types/messages";
import { useArtifactPolicyContext } from "@/features/chat/hooks/ArtifactPolicyContext";
import type { ArtifactPathCandidate } from "@/features/chat/lib/artifactPathPolicy";

interface ToolCallAdapterProps {
  name: string;
  arguments: Record<string, unknown>;
  status: ToolCallStatus;
  result?: string;
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

function ArtifactActions({
  args,
  name,
  result,
}: {
  args: Record<string, unknown>;
  name: string;
  result?: string;
}) {
  const { t } = useTranslation(["chat", "common"]);
  const [moreOutputsOpen, setMoreOutputsOpen] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);
  const { resolveToolCardDisplay, pathExists, openResolvedPath } =
    useArtifactPolicyContext();

  const display = useMemo(
    () => resolveToolCardDisplay(args, name, result),
    [args, name, resolveToolCardDisplay, result],
  );

  if (display.role !== "primary_host" || !display.primaryCandidate) return null;

  const openCandidate = async (
    candidate: ArtifactPathCandidate,
    allowFallback: boolean,
  ) => {
    const candidates = allowFallback
      ? [
          candidate,
          ...display.secondaryCandidates.filter((c) => c.id !== candidate.id),
        ]
      : [candidate];

    try {
      setOpenError(null);
      for (const c of candidates) {
        const exists = await pathExists(c.resolvedPath);
        if (c.allowed && exists) {
          await openResolvedPath(c.resolvedPath);
          return;
        }
      }
      for (const c of candidates) {
        const exists = await pathExists(c.resolvedPath);
        if (exists && !c.allowed) {
          setOpenError(c.blockedReason || t("tools.pathOutsideRoots"));
          return;
        }
      }
      const firstAllowed = candidates.find((c) => c.allowed);
      if (firstAllowed) {
        setOpenError(
          t("tools.fileNotFound", { path: firstAllowed.resolvedPath }),
        );
        return;
      }
      setOpenError(candidate.blockedReason || t("tools.pathOutsideRoots"));
    } catch (error) {
      setOpenError(error instanceof Error ? error.message : String(error));
    }
  };

  const primary = display.primaryCandidate;
  const kindLabel: Record<string, string> = {
    file: t("tools.openFile"),
    folder: t("tools.openFolder"),
    path: t("tools.openPath"),
  };

  return (
    <div className="mt-1.5 ml-1 space-y-1.5">
      <Button
        type="button"
        variant="outline-flat"
        onClick={() => void openCandidate(primary, true)}
        className={cn(
          "h-auto max-w-full justify-start rounded-md px-2.5 py-1 text-xs",
          primary.allowed
            ? "border-accent/45 bg-background text-accent-foreground hover:bg-accent/55"
            : "cursor-not-allowed border-red-500/30 bg-red-500/[0.04] text-red-500/70",
        )}
        disabled={!primary.allowed}
        title={primary.resolvedPath}
      >
        <FolderOpen className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate">
          {kindLabel[primary.kind] ?? t("common:actions.open")}
        </span>
        <span className="truncate text-[10px] text-muted-foreground">
          {primary.rawPath || primary.resolvedPath}
        </span>
      </Button>
      {!primary.allowed && primary.blockedReason && (
        <p className="text-[11px] text-destructive ml-1">
          {primary.blockedReason}
        </p>
      )}

      {display.secondaryCandidates.length > 0 && (
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
              count: display.secondaryCandidates.length,
            })}
          </button>
          {moreOutputsOpen && (
            <div className="space-y-1.5 pl-4">
              {display.secondaryCandidates.map((candidate) => (
                <div key={candidate.id} className="space-y-0.5">
                  <Button
                    type="button"
                    variant="outline-flat"
                    onClick={() => void openCandidate(candidate, false)}
                    className={cn(
                      "h-auto max-w-full justify-start rounded-md px-2 py-1 text-[11px]",
                      candidate.allowed
                        ? "border-border bg-background text-muted-foreground hover:bg-accent hover:text-foreground"
                        : "cursor-not-allowed border-red-500/20 bg-red-500/[0.03] text-red-500/70",
                    )}
                    disabled={!candidate.allowed}
                    title={candidate.resolvedPath}
                  >
                    <FolderOpen className="h-3 w-3 shrink-0" />
                    <span className="truncate">
                      {kindLabel[candidate.kind] ?? t("common:actions.open")}
                    </span>
                    <span className="truncate text-[10px] text-muted-foreground">
                      {candidate.rawPath || candidate.resolvedPath}
                    </span>
                  </Button>
                  {!candidate.allowed && candidate.blockedReason && (
                    <p className="text-[11px] text-destructive">
                      {candidate.blockedReason}
                    </p>
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
  result,
  isError,
  startedAt,
  open,
  onOpenChange,
}: ToolCallAdapterProps) {
  const elapsed = useElapsedTime(status, startedAt);
  const state = toolStatusMap[status];

  const elapsedSeconds =
    status === "executing" && elapsed >= 3 ? elapsed : undefined;

  return (
    <div>
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
          />
        </ToolContent>
      </Tool>
      <ArtifactActions args={args} name={name} result={result} />
    </div>
  );
}
