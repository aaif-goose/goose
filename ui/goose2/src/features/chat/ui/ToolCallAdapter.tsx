import { useState, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useControllableState } from "@radix-ui/react-use-controllable-state";
import { FolderOpen, ChevronRight, FileText } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { CodeBlock } from "@/shared/ui/ai-elements/code-block";
import {
  Tool,
  ToolHeader,
  ToolContent,
  ToolInput,
  ToolOutput,
  ToolSection,
  ToolSurface,
} from "@/shared/ui/ai-elements/tool";
import { toolStatusMap } from "../lib/toolStatusMap";
import type {
  ToolCallKind,
  ToolCallLocation,
  ToolCallStructuredContent,
  ToolCallStatus,
} from "@/shared/types/messages";
import { useArtifactPolicyContext } from "@/features/chat/hooks/ArtifactPolicyContext";
import type { ArtifactPathCandidate } from "@/features/chat/lib/artifactPathPolicy";
import {
  dedupeToolLocations,
  getToolInputSummaryRows,
  getToolLocationSubtitle,
  getToolLocationTitle,
  isFileOrientedToolCall,
} from "../lib/toolCallPresentation";

interface ToolCallAdapterProps {
  name: string;
  arguments: Record<string, unknown>;
  kind?: ToolCallKind;
  locations?: ToolCallLocation[];
  status: ToolCallStatus;
  result?: string;
  content?: ToolCallStructuredContent[];
  rawOutput?: unknown;
  isError?: boolean;
  /** Epoch ms when the tool call started executing. */
  startedAt?: number;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  showStatusBadge?: boolean;
  fitWidth?: boolean;
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

function InputSummary({
  rows,
  isOpen,
}: {
  rows: ReturnType<typeof getToolInputSummaryRows>;
  isOpen: boolean;
}) {
  const commandCodeBlockClasses =
    "rounded-none border-0 bg-transparent shadow-none [&>div]:overflow-hidden [&_pre]:m-0 [&_pre]:bg-transparent [&_pre]:p-0 [&_pre]:whitespace-pre-wrap [&_pre]:break-words [&_pre]:text-[12px] [&_pre]:leading-5 [&_code]:font-mono [&_code]:text-[12px] [&_code]:leading-5";

  if (rows.length === 0) {
    return null;
  }

  return (
    <div className="space-y-2">
      {rows.map((row) => (
        <div key={`${row.label}-${row.value}`} className="max-w-full">
          {row.renderAs === "bash" ? (
            <div className="space-y-0.5">
              <div className="text-[11px] uppercase tracking-wide text-muted-foreground">
                {row.label}
              </div>
              <CodeBlock
                code={row.value}
                language="bash"
                data-tool-command-preview={!isOpen ? "" : undefined}
                className={cn(
                  commandCodeBlockClasses,
                  !isOpen && "[&_pre]:line-clamp-3 [&_pre]:overflow-hidden",
                )}
              />
            </div>
          ) : (
            <div className="space-y-0.5">
              <div className="text-[11px] uppercase tracking-wide text-muted-foreground">
                {row.label}
              </div>
              <div
                title={row.title}
                className={cn(
                  "block min-w-0 break-words text-[13px] text-foreground",
                  row.monospace && "font-mono text-[12px]",
                )}
              >
                {row.value}
              </div>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function ToolLocations({ locations }: { locations: ToolCallLocation[] }) {
  const { t } = useTranslation("chat");
  const { pathExists, openResolvedPath } = useArtifactPolicyContext();
  const [openError, setOpenError] = useState<string | null>(null);

  const openLocation = async (path: string) => {
    try {
      setOpenError(null);
      const exists = await pathExists(path);
      if (!exists) {
        setOpenError(t("tools.fileNotFound", { path }));
        return;
      }
      await openResolvedPath(path);
    } catch (error) {
      setOpenError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <ToolSection label="Files">
      <div className="flex flex-wrap gap-1.5">
        {locations.map((location) => (
          <Button
            key={`${location.path}:${location.line ?? ""}`}
            type="button"
            variant="outline-flat"
            onClick={() => void openLocation(location.path)}
            className="inline-flex h-auto max-w-full items-center justify-start rounded-full px-2.5 py-1 text-xs"
            title={getToolLocationSubtitle(location)}
          >
            <FileText className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate">{getToolLocationTitle(location)}</span>
          </Button>
        ))}
      </div>
      {openError ? (
        <p className="text-[11px] text-destructive">{openError}</p>
      ) : null}
    </ToolSection>
  );
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
    <div className="space-y-1.5">
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
        <p className="ml-1 text-[11px] text-destructive">
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

function splitHeaderTitle(name: string, fileLabel: string) {
  const index = name.toLowerCase().lastIndexOf(fileLabel.toLowerCase());
  if (index === -1) {
    return null;
  }

  return {
    prefix: name.slice(0, index),
    fileLabel: name.slice(index, index + fileLabel.length),
    suffix: name.slice(index + fileLabel.length),
  };
}

export function ToolCallAdapter({
  name,
  arguments: args,
  kind,
  locations,
  status,
  result,
  rawOutput,
  isError,
  startedAt,
  open,
  onOpenChange,
  showStatusBadge = true,
  fitWidth = false,
}: ToolCallAdapterProps) {
  const elapsed = useElapsedTime(status, startedAt);
  const state = toolStatusMap[status];
  const [isToolOpen, setIsToolOpen] = useControllableState({
    prop: open,
    defaultProp: false,
    onChange: onOpenChange,
  });
  const summaryRows = useMemo(
    () => getToolInputSummaryRows({ name, kind, locations, arguments: args }),
    [args, kind, locations, name],
  );
  const visibleLocations = useMemo(
    () => dedupeToolLocations(locations),
    [locations],
  );
  const isFileOriented = isFileOrientedToolCall({
    kind,
    locations: visibleLocations,
    arguments: args,
  });
  const { openResolvedPath } = useArtifactPolicyContext();
  const rawResult =
    result ??
    (typeof rawOutput === "string"
      ? rawOutput
      : rawOutput != null
        ? JSON.stringify(rawOutput, null, 2)
        : undefined);

  const elapsedSeconds =
    status === "executing" && elapsed >= 3 ? elapsed : undefined;
  const pathSummaryRow = summaryRows.find((row) => row.label === "Path");
  const headerFileLabel = pathSummaryRow?.value;
  const headerFilePath = pathSummaryRow?.title ?? pathSummaryRow?.value;
  const headerTitleParts =
    headerFileLabel && headerFilePath
      ? splitHeaderTitle(name, headerFileLabel)
      : null;
  const canOpenHeaderFile = Boolean(headerFilePath && headerTitleParts);

  return (
    <div className={cn(fitWidth && "inline-flex max-w-full flex-col")}>
      <Tool
        open={isToolOpen}
        onOpenChange={setIsToolOpen}
        className={cn(fitWidth && "inline-flex w-auto max-w-full flex-col")}
      >
        <ToolHeader
          type="dynamic-tool"
          toolName={name}
          title={
            headerTitleParts ? (
              <>
                <span data-tool-title-prefix>{headerTitleParts.prefix}</span>
                <button
                  type="button"
                  data-clickable-file
                  onClick={(event) => {
                    event.stopPropagation();
                    if (!headerFilePath) {
                      return;
                    }
                    void openResolvedPath(headerFilePath).catch(() => {});
                  }}
                  onKeyDown={(event) => {
                    event.stopPropagation();
                  }}
                  title={headerFilePath}
                  aria-label={`Open ${headerTitleParts.fileLabel}`}
                  className="inline truncate text-foreground underline-offset-2 hover:underline"
                >
                  {headerTitleParts.fileLabel}
                </button>
                <span>{headerTitleParts.suffix}</span>
              </>
            ) : (
              name
            )
          }
          splitTrigger={canOpenHeaderFile}
          state={state}
          showIcon={false}
          showStatusBadge={showStatusBadge}
          elapsedSeconds={elapsedSeconds}
          layout={fitWidth ? "fit" : "fill"}
        />
        <ToolContent>
          <ToolSurface tone="muted" className="overflow-hidden bg-muted">
            <ToolInput
              input={args}
              showLabel={false}
              embedded
              summary={({ isOpen }) => (
                <InputSummary rows={summaryRows} isOpen={isOpen} />
              )}
            />
            {isFileOriented ? (
              <ToolOutput
                output={isError ? undefined : rawResult}
                errorText={isError ? result : undefined}
                showLabel={false}
                embedded
                tone="muted"
              />
            ) : (
              <ToolOutput
                output={isError ? undefined : result}
                errorText={isError ? result : undefined}
                showLabel={false}
                embedded
              />
            )}
          </ToolSurface>
          {isFileOriented && visibleLocations.length > 1 ? (
            <ToolLocations locations={visibleLocations} />
          ) : null}
          <ArtifactActions args={args} name={name} result={result} />
        </ToolContent>
      </Tool>
    </div>
  );
}
