import { useState } from "react";
import { Check, ChevronRight } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { ToolCallAdapter } from "./ToolCallAdapter";
import { toolStatusMap } from "../lib/toolStatusMap";
import {
  Tool,
  ToolContent,
  ToolHeader,
  ToolStatusIcon,
} from "@/shared/ui/ai-elements/tool";
import type {
  ToolCallStatus,
  ToolRequestContent,
  ToolResponseContent,
} from "@/shared/types/messages";

export interface ToolChainItem {
  key: string;
  request?: ToolRequestContent;
  response?: ToolResponseContent;
}

interface ToolItemGroup {
  key: string;
  chainId?: string;
  items: ToolChainItem[];
}

const INTERNAL_TOOL_PREFIXES = new Set([
  "awk",
  "bash",
  "cat",
  "chmod",
  "cp",
  "echo",
  "find",
  "grep",
  "head",
  "ls",
  "mv",
  "open",
  "pip",
  "pip3",
  "python",
  "python3",
  "rm",
  "sed",
  "sh",
  "tail",
  "wc",
  "which",
  "zsh",
]);

function getToolItemChainId(item: ToolChainItem): string | undefined {
  return item.request?.chainId ?? item.response?.chainId;
}

function getToolItemName(item: ToolChainItem): string {
  return item.request?.name || item.response?.name || "Tool result";
}

function getToolItemChainSummary(item: ToolChainItem): string | undefined {
  return item.response?.chainSummary ?? item.request?.chainSummary;
}

function getToolItemStatus(item: ToolChainItem): ToolCallStatus {
  if (item.response) {
    return item.response.isError ? "error" : "completed";
  }
  return item.request?.status ?? "completed";
}

function isLowSignalToolStep(item: ToolChainItem): boolean {
  if (getToolItemStatus(item) !== "completed") {
    return false;
  }
  if (item.response?.isError) {
    return false;
  }

  const name = getToolItemName(item).trim();
  if (!name) return false;

  const lower = name.toLowerCase();
  const firstToken = lower.split(/\s+/)[0];
  if (INTERNAL_TOOL_PREFIXES.has(firstToken)) {
    return true;
  }
  if (name.length > 88) {
    return true;
  }
  return (
    lower.includes("&&") ||
    lower.includes("||") ||
    lower.includes("2>&1") ||
    lower.includes("|")
  );
}

function partitionToolSteps(toolItems: ToolChainItem[]) {
  if (toolItems.length <= 3) {
    return { primaryItems: toolItems, hiddenItems: [] as ToolChainItem[] };
  }

  const primaryItems: ToolChainItem[] = [];
  const hiddenItems: ToolChainItem[] = [];

  for (const item of toolItems) {
    if (isLowSignalToolStep(item)) {
      hiddenItems.push(item);
      continue;
    }
    primaryItems.push(item);
  }

  if (primaryItems.length === 0) {
    return { primaryItems: toolItems, hiddenItems: [] as ToolChainItem[] };
  }

  if (hiddenItems.length < 2) {
    return { primaryItems: toolItems, hiddenItems: [] as ToolChainItem[] };
  }

  return { primaryItems, hiddenItems };
}

function groupToolItems(toolItems: ToolChainItem[]): ToolItemGroup[] {
  const groups: ToolItemGroup[] = [];

  for (const item of toolItems) {
    const chainId = getToolItemChainId(item);
    const currentGroup = groups[groups.length - 1];

    if (currentGroup && currentGroup.chainId === chainId) {
      currentGroup.items.push(item);
      continue;
    }

    groups.push({
      key: chainId ? `chain-${chainId}-${item.key}` : `group-${item.key}`,
      chainId,
      items: [item],
    });
  }

  return groups;
}

function getToolGroupTitle(toolItems: ToolChainItem[]): string {
  for (let index = toolItems.length - 1; index >= 0; index -= 1) {
    const chainSummary = getToolItemChainSummary(toolItems[index]);
    if (chainSummary) {
      return chainSummary;
    }
  }

  const displayItem =
    toolItems.find((item) => !isLowSignalToolStep(item)) ?? toolItems[0];
  return getToolItemName(displayItem);
}

function getToolGroupStatus(toolItems: ToolChainItem[]): ToolCallStatus {
  if (toolItems.some((item) => getToolItemStatus(item) === "executing")) {
    return "executing";
  }
  if (toolItems.some((item) => getToolItemStatus(item) === "pending")) {
    return "pending";
  }
  if (toolItems.some((item) => getToolItemStatus(item) === "error")) {
    return "error";
  }
  if (toolItems.some((item) => getToolItemStatus(item) === "stopped")) {
    return "stopped";
  }
  return "completed";
}

function formatToolGroupTitle(
  title: string,
  stepCount: number,
  status: ToolCallStatus,
): string {
  if (title === "working" || status === "pending" || status === "executing") {
    return `working through ${stepCount} ${stepCount === 1 ? "step" : "steps"}`;
  }

  return `${title} (${stepCount} ${stepCount === 1 ? "step" : "steps"})`;
}

function shouldShowGroupedChain(group: ToolItemGroup): boolean {
  return Boolean(group.chainId);
}

export function ToolChainCards({ toolItems }: { toolItems: ToolChainItem[] }) {
  const [showInternalStepGroups, setShowInternalStepGroups] = useState<
    Set<string>
  >(new Set());
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());
  const [expandedChainOverrides, setExpandedChainOverrides] = useState<
    Record<string, boolean>
  >({});
  const groups = groupToolItems(toolItems);

  const handleOpenChange = (key: string, open: boolean) => {
    setExpandedKeys((prev) => {
      const next = new Set(prev);
      if (open) {
        next.add(key);
      } else {
        next.delete(key);
      }
      return next;
    });
  };

  const handleShowInternalStepsChange = (groupKey: string) => {
    setShowInternalStepGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupKey)) {
        next.delete(groupKey);
      } else {
        next.add(groupKey);
      }
      return next;
    });
  };

  const handleChainOpenChange = (groupKey: string, open: boolean) => {
    setExpandedChainOverrides((prev) => ({
      ...prev,
      [groupKey]: open,
    }));
  };

  const renderToolItem = (
    item: ToolChainItem,
    itemIndex: number,
    totalItems: number,
  ) => {
    const name = getToolItemName(item);
    const status = getToolItemStatus(item);
    const { request, response } = item;
    const isLast = itemIndex === totalItems - 1;
    const isCompleted = status === "completed";

    return (
      <div key={item.key} className="flex max-w-full items-start gap-2.5">
        <div className="relative flex w-4 shrink-0 justify-center self-stretch">
          <div className="pointer-events-none absolute top-0 left-1/2 h-0.5 w-px -translate-x-1/2 bg-border" />
          {!isLast && (
            <div className="pointer-events-none absolute top-[1.375rem] bottom-[-0.5rem] left-1/2 w-px -translate-x-1/2 bg-border" />
          )}
          <div className="relative z-10 mt-1 flex h-4 w-4 items-center justify-center rounded-full bg-background ring-2 ring-background">
            {isCompleted ? (
              <Check
                aria-hidden="true"
                className="size-4 shrink-0 text-muted-foreground"
              />
            ) : (
              <ToolStatusIcon
                status={toolStatusMap[status]}
                className={cn(
                  status !== "error" &&
                    status !== "stopped" &&
                    "text-muted-foreground",
                )}
              />
            )}
          </div>
        </div>
        <div className="inline-flex min-w-0 max-w-full flex-col">
          <ToolCallAdapter
            name={name}
            arguments={request?.arguments ?? {}}
            kind={response?.kind ?? request?.kind}
            locations={response?.locations ?? request?.locations}
            status={status}
            result={response?.result}
            content={response?.content ?? request?.content}
            rawOutput={response?.rawOutput ?? request?.rawOutput}
            isError={response?.isError}
            startedAt={request?.startedAt}
            open={expandedKeys.has(item.key)}
            onOpenChange={(open) => handleOpenChange(item.key, open)}
            showStatusBadge={false}
            fitWidth
          />
        </div>
      </div>
    );
  };

  const renderToolItems = (
    items: ToolChainItem[],
    startIndex = 0,
    totalItems = items.length,
  ) =>
    items.map((item, index) =>
      renderToolItem(item, startIndex + index, totalItems),
    );

  const renderToolStepList = (groupKey: string, items: ToolChainItem[]) => {
    const { primaryItems, hiddenItems } = partitionToolSteps(items);
    const showInternalSteps = showInternalStepGroups.has(groupKey);
    const visibleItemCount = showInternalSteps
      ? primaryItems.length + hiddenItems.length
      : primaryItems.length;

    return (
      <>
        {renderToolItems(primaryItems, 0, visibleItemCount)}

        {hiddenItems.length > 0 && (
          <div className="ml-6 flex flex-col items-start gap-1.5">
            <button
              type="button"
              onClick={() => handleShowInternalStepsChange(groupKey)}
              className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-muted-foreground"
            >
              <ChevronRight
                className={cn(
                  "h-3 w-3 transition-transform",
                  showInternalSteps && "rotate-90",
                )}
              />
              {showInternalSteps
                ? `Hide internal steps (${hiddenItems.length})`
                : `Show internal steps (${hiddenItems.length})`}
            </button>

            {showInternalSteps &&
              renderToolItems(
                hiddenItems,
                primaryItems.length,
                visibleItemCount,
              )}
          </div>
        )}
      </>
    );
  };

  return (
    <div className="my-1 flex w-full flex-col items-start gap-2">
      {groups.map((group) => {
        if (!shouldShowGroupedChain(group)) {
          return (
            <div
              key={group.key}
              className="flex w-full flex-col items-start gap-2"
            >
              {renderToolStepList(group.key, group.items)}
            </div>
          );
        }

        const title = getToolGroupTitle(group.items);
        const status = getToolGroupStatus(group.items);
        const stepCount = group.items.length;
        const headerTitle = formatToolGroupTitle(title, stepCount, status);
        const open =
          expandedChainOverrides[group.key] ??
          group.items.some((item) => {
            const itemStatus = getToolItemStatus(item);
            return itemStatus === "pending" || itemStatus === "executing";
          });

        return (
          <Tool
            key={group.key}
            open={open}
            onOpenChange={(nextOpen) =>
              handleChainOpenChange(group.key, nextOpen)
            }
            className="inline-flex w-auto max-w-full flex-col"
          >
            <ToolHeader
              type="dynamic-tool"
              toolName={title}
              title={headerTitle}
              state={toolStatusMap[status]}
              showIcon={false}
              layout="fit"
            />
            <ToolContent className="space-y-2">
              <div className="flex flex-col items-start gap-2">
                {renderToolStepList(group.key, group.items)}
              </div>
            </ToolContent>
          </Tool>
        );
      })}
    </div>
  );
}
