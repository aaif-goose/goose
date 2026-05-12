import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ChevronRight, FolderOpen } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { CodeBlock } from "@/shared/ui/ai-elements/code-block";
import {
  Tool,
  ToolContent,
  ToolHeader,
  ToolInput,
  ToolOutput,
  ToolSurface,
} from "@/shared/ui/ai-elements/tool";
import { toolStatusMap } from "../lib/toolStatusMap";
import {
  getToolInputSummaryRows,
  isHoistableText,
  isStringifiedCopyOfStructured,
  type ToolInputSummaryRow,
} from "@/features/chat/lib/toolCallPresentation";
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
  /** When false, the chevron-side status badge is hidden (used inside chains). */
  showStatusBadge?: boolean;
  /** When false, hides the trailing disclosure chevron in the header. */
  showChevron?: boolean;
  /** When true, the card sizes to its content rather than filling its parent. */
  fitWidth?: boolean;
}

function useElapsedTime(status: ToolCallStatus, startedAt?: number) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (status === "in_progress") {
      const origin = startedAt ?? Date.now();
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

const COMMAND_PREVIEW_CODEBLOCK_CLASSES =
  "rounded-none border-0 bg-transparent shadow-none [&>div]:overflow-hidden [&_pre]:m-0 [&_pre]:bg-transparent [&_pre]:p-0 [&_pre]:whitespace-pre-wrap [&_pre]:break-words [&_pre]:text-[12px] [&_pre]:leading-5 [&_code]:font-mono [&_code]:text-[12px] [&_code]:leading-5";

function InputSummary({
  rows,
  isOpen,
}: {
  rows: ToolInputSummaryRow[];
  isOpen: boolean;
}) {
  const { t } = useTranslation("chat");
  if (rows.length === 0) return null;

  return (
    <dl className="space-y-1.5">
      {rows.map((row) => {
        const label = t(`tools.inputSummary.${row.kind}`);
        const key = `${row.kind}:${row.value}`;
        if (row.renderAs === "bash") {
          return (
            <div key={key} className="space-y-0.5">
              <dt className="text-[10px] uppercase tracking-wide text-muted-foreground">
                {label}
              </dt>
              <dd>
                <CodeBlock
                  code={row.value}
                  language="bash"
                  data-tool-command-preview={!isOpen ? "" : undefined}
                  className={cn(
                    COMMAND_PREVIEW_CODEBLOCK_CLASSES,
                    !isOpen && "[&_pre]:line-clamp-3 [&_pre]:overflow-hidden",
                  )}
                />
              </dd>
            </div>
          );
        }
        return (
          <div key={key} className="flex items-baseline gap-2">
            <dt className="shrink-0 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
              {label}
            </dt>
            <dd
              className={cn(
                "min-w-0 truncate text-[12px] text-foreground",
                row.monospace && "font-mono",
              )}
              title={row.title ?? row.value}
            >
              {row.value}
            </dd>
          </div>
        );
      })}
    </dl>
  );
}

function splitHeaderTitleByPath(name: string, fileLabel: string) {
  const index = name.toLowerCase().lastIndexOf(fileLabel.toLowerCase());
  if (index === -1) return null;
  return {
    prefix: name.slice(0, index),
    fileLabel: name.slice(index, index + fileLabel.length),
    suffix: name.slice(index + fileLabel.length),
  };
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
  showStatusBadge = true,
  showChevron = true,
  fitWidth = false,
}: ToolCallAdapterProps) {
  const { t } = useTranslation("chat");
  const elapsed = useElapsedTime(status, startedAt);
  const state = toolStatusMap[status];
  const summaryRows = useMemo(
    () => getToolInputSummaryRows({ name, arguments: args }),
    [args, name],
  );
  const elapsedSeconds =
    status === "in_progress" && elapsed >= 3 ? elapsed : undefined;

  const { resolveMarkdownHref, openResolvedPath } = useArtifactPolicyContext();

  const pathRow = summaryRows.find((row) => row.kind === "path");
  const headerFileLabel = pathRow?.value;
  const headerFilePath = pathRow?.title ?? pathRow?.value;
  const headerTitleParts =
    headerFileLabel && headerFilePath
      ? splitHeaderTitleByPath(name, headerFileLabel)
      : null;
  const headerFileCandidate = useMemo(
    () => (headerFilePath ? resolveMarkdownHref(headerFilePath) : null),
    [headerFilePath, resolveMarkdownHref],
  );
  const canOpenHeaderFile = Boolean(headerTitleParts && headerFileCandidate);

  const hasStructuredArgs = Object.keys(args).length > 0;
  const hasOutput = Boolean(result);
  const hasStructuredContent = !isError && structuredContent !== undefined;

  // De-dupe + title-hoisting matrix: when both a text result and structured
  // content are present, decide whether the text is a redundant stringified
  // copy of the structured payload (hide), short enough to hoist into the
  // header subtitle (lift), or worth rendering in the body alongside the
  // structured block (keep).
  const textIsStringifiedCopy =
    hasOutput &&
    hasStructuredContent &&
    isStringifiedCopyOfStructured(result, structuredContent);
  const canHoistResultIntoHeader =
    hasOutput &&
    hasStructuredContent &&
    !textIsStringifiedCopy &&
    !headerTitleParts &&
    isHoistableText(result);
  const showResultBody =
    hasOutput && !textIsStringifiedCopy && !canHoistResultIntoHeader;

  const headerTitle: ReactNode = headerTitleParts ? (
    <>
      <span data-tool-title-prefix>{headerTitleParts.prefix}</span>
      {canOpenHeaderFile ? (
        <button
          type="button"
          data-clickable-file
          onClick={(event) => {
            event.stopPropagation();
            if (!headerFileCandidate) return;
            void openResolvedPath(headerFileCandidate.resolvedPath).catch(
              () => {},
            );
          }}
          onKeyDown={(event) => {
            event.stopPropagation();
          }}
          title={headerFileCandidate?.resolvedPath ?? headerFilePath}
          aria-label={t("tools.openNamed", {
            name: headerTitleParts.fileLabel,
          })}
          className="inline truncate text-foreground underline-offset-2 hover:underline"
        >
          {headerTitleParts.fileLabel}
        </button>
      ) : (
        <span>{headerTitleParts.fileLabel}</span>
      )}
      <span>{headerTitleParts.suffix}</span>
    </>
  ) : canHoistResultIntoHeader ? (
    <>
      <span>{name}</span>
      <span aria-hidden="true" className="text-muted-foreground">
        {" · "}
      </span>
      <span data-tool-title-hoisted className="truncate text-foreground">
        {(result ?? "").trim()}
      </span>
    </>
  ) : (
    name
  );

  const showCombinedSurface = summaryRows.length > 0 || hasStructuredArgs;

  return (
    <div className="w-full min-w-0 max-w-full">
      <Tool open={open} onOpenChange={onOpenChange}>
        <ToolHeader
          type="dynamic-tool"
          toolName={name}
          title={headerTitle}
          state={state}
          showIcon={false}
          showStatusBadge={showStatusBadge}
          showChevron={showChevron}
          splitTrigger={canOpenHeaderFile}
          layout={fitWidth ? "fit" : "fill"}
          elapsedSeconds={elapsedSeconds}
        />
        <ToolContent>
          {showCombinedSurface ? (
            <ToolSurface tone="muted" className="bg-muted">
              <ToolInput
                input={args}
                showLabel={false}
                embedded
                summary={({ isOpen }) => (
                  <InputSummary rows={summaryRows} isOpen={isOpen} />
                )}
              />
              {showResultBody && (
                <ToolOutput
                  output={isError ? undefined : result}
                  errorText={isError ? result : undefined}
                  showLabel={false}
                  embedded
                  embeddedMaxHeightClass="max-h-32"
                />
              )}
              {hasStructuredContent && (
                <ToolOutput
                  output={structuredContent}
                  errorText={undefined}
                  showLabel={false}
                  embedded
                  embeddedMaxHeightClass="max-h-32"
                />
              )}
            </ToolSurface>
          ) : (
            <>
              {showResultBody && (
                <ToolOutput
                  output={isError ? undefined : result}
                  errorText={isError ? result : undefined}
                  contentClassName="max-h-[28rem] overflow-y-auto"
                />
              )}
              {hasStructuredContent && (
                <ToolOutput
                  output={structuredContent}
                  errorText={undefined}
                  label={t("tools.structuredContent")}
                  contentClassName="max-h-[28rem] overflow-y-auto"
                />
              )}
            </>
          )}
        </ToolContent>
      </Tool>
      <ArtifactActions locations={locations} />
    </div>
  );
}
