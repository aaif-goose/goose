import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronRight } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { ToolCallAdapter } from "./ToolCallAdapter";
import {
  getChainAggregateStatus,
  getToolItemName,
  getToolItemStatus,
  shouldRenderAsGroupedChain,
  type ToolChainItem,
} from "@/features/chat/lib/toolChainGrouping";
import { summarizeToolChainSteps } from "@/features/chat/lib/toolChainSummary";

export type { ToolChainItem };

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

export function ToolChainCards({ toolItems }: { toolItems: ToolChainItem[] }) {
  const { t } = useTranslation("chat");
  const [showInternalSteps, setShowInternalSteps] = useState(false);
  const [chainExpanded, setChainExpanded] = useState(true);
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());
  const { primaryItems, hiddenItems } = partitionToolSteps(toolItems);
  const grouped = shouldRenderAsGroupedChain(toolItems);
  const aggregateStatus = getChainAggregateStatus(toolItems);
  const summary = summarizeToolChainSteps(primaryItems);
  const isActiveChain =
    aggregateStatus === "executing" || aggregateStatus === "pending";

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

  const renderToolItem = (item: ToolChainItem) => {
    const name = getToolItemName(item);
    const status = getToolItemStatus(item);
    const { request, response } = item;

    return (
      <ToolCallAdapter
        key={item.key}
        name={name}
        arguments={request?.arguments ?? {}}
        status={status}
        locations={request?.locations}
        result={response?.result}
        structuredContent={response?.structuredContent}
        isError={response?.isError}
        startedAt={request?.startedAt}
        open={expandedKeys.has(item.key)}
        onOpenChange={(open) => handleOpenChange(item.key, open)}
      />
    );
  };

  const items = (
    <div className="flex w-full min-w-0 max-w-full flex-col items-start gap-3">
      {primaryItems.map((item) => renderToolItem(item))}

      {hiddenItems.length > 0 && (
        <div className="ml-1 flex flex-col items-start gap-1.5">
          <button
            type="button"
            onClick={() => setShowInternalSteps((prev) => !prev)}
            className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-muted-foreground"
          >
            <ChevronRight
              className={cn(
                "h-3 w-3 transition-transform",
                showInternalSteps && "rotate-90",
              )}
            />
            {showInternalSteps
              ? t("tool_chain.internalSteps.hide", {
                  count: hiddenItems.length,
                })
              : t("tool_chain.internalSteps.show", {
                  count: hiddenItems.length,
                })}
          </button>

          {showInternalSteps && hiddenItems.map((item) => renderToolItem(item))}
        </div>
      )}
    </div>
  );

  if (!grouped) {
    return <div className="my-1">{items}</div>;
  }

  const labelText = isActiveChain
    ? t("tool_chain.summary.active")
    : t(summary.titleKey);
  const headerText = isActiveChain
    ? t("tool_chain.title.active", { count: toolItems.length })
    : t("tool_chain.title.labeled", {
        label: labelText,
        count: toolItems.length,
      });

  return (
    <section
      className="my-1 flex w-full flex-col gap-2 rounded-lg border border-border/60 bg-muted/30 p-3"
      data-role="tool-chain-card"
      data-status={aggregateStatus}
    >
      <button
        type="button"
        onClick={() => setChainExpanded((prev) => !prev)}
        aria-expanded={chainExpanded}
        className="inline-flex items-center gap-1.5 self-start text-xs text-muted-foreground hover:text-foreground"
      >
        <ChevronRight
          aria-hidden="true"
          className={cn(
            "h-3.5 w-3.5 transition-transform",
            chainExpanded && "rotate-90",
          )}
        />
        <span>{headerText}</span>
      </button>
      {chainExpanded && items}
    </section>
  );
}
