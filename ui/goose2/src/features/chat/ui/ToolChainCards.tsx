import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronRight,
  CircleIcon,
  ClockIcon,
  XCircleIcon,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
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
import type { ToolCallStatus } from "@/shared/types/messages";

export type { ToolChainItem };

const STEP_BULLET_ICON: Record<ToolCallStatus, LucideIcon> = {
  pending: CircleIcon,
  executing: ClockIcon,
  completed: Check,
  error: XCircleIcon,
  stopped: XCircleIcon,
};

const STEP_BULLET_CLASS: Record<ToolCallStatus, string> = {
  pending: "text-muted-foreground/70",
  executing: "text-muted-foreground animate-pulse",
  completed: "text-muted-foreground",
  error: "text-red-600",
  stopped: "text-orange-600",
};

function ChainStepRail({
  status,
  isFirst,
  isLast,
}: {
  status: ToolCallStatus;
  isFirst: boolean;
  isLast: boolean;
}) {
  const Icon = STEP_BULLET_ICON[status];
  return (
    <div
      aria-hidden="true"
      className="relative flex w-4 shrink-0 justify-center self-stretch"
    >
      {!isFirst && (
        <div className="pointer-events-none absolute top-0 bottom-1/2 left-1/2 w-px -translate-x-1/2 bg-border" />
      )}
      {!isLast && (
        <div className="pointer-events-none absolute top-1/2 bottom-0 left-1/2 w-px -translate-x-1/2 bg-border" />
      )}
      <div className="relative z-10 mt-1 flex h-4 w-4 items-center justify-center rounded-full bg-background ring-2 ring-background">
        <Icon className={cn("size-3.5 shrink-0", STEP_BULLET_CLASS[status])} />
      </div>
    </div>
  );
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

  const renderToolItem = (
    item: ToolChainItem,
    index: number,
    total: number,
    options: { withRail: boolean },
  ) => {
    const name = getToolItemName(item);
    const status = getToolItemStatus(item);
    const { request, response } = item;

    const adapter = (
      <ToolCallAdapter
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

    if (!options.withRail) {
      return <div key={item.key}>{adapter}</div>;
    }

    return (
      <div
        key={item.key}
        data-role="tool-chain-step"
        className="flex max-w-full items-stretch gap-2.5"
      >
        <ChainStepRail
          status={status}
          isFirst={index === 0}
          isLast={index === total - 1}
        />
        <div className="min-w-0 flex-1 pb-1">{adapter}</div>
      </div>
    );
  };

  if (!grouped) {
    return (
      <div className="my-1 flex w-full min-w-0 max-w-full flex-col gap-3">
        {primaryItems.map((item, index) =>
          renderToolItem(item, index, primaryItems.length, { withRail: false }),
        )}
      </div>
    );
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

  const hasHiddenDisclosure = hiddenItems.length > 0;
  const railRowCount =
    primaryItems.length +
    (hasHiddenDisclosure ? 1 : 0) +
    (showInternalSteps ? hiddenItems.length : 0);
  const disclosureIndex = primaryItems.length;
  const firstHiddenIndex = disclosureIndex + 1;

  return (
    <section
      className="my-1 flex w-full min-w-0 max-w-full flex-col gap-2"
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

      {chainExpanded && (
        <div className="flex flex-col">
          {primaryItems.map((item, index) =>
            renderToolItem(item, index, railRowCount, { withRail: true }),
          )}

          {hasHiddenDisclosure && (
            <div
              data-role="tool-chain-internal-disclosure"
              className="flex max-w-full items-stretch gap-2.5"
            >
              <ChainStepRail
                status="completed"
                isFirst={disclosureIndex === 0}
                isLast={disclosureIndex === railRowCount - 1}
              />
              <div className="min-w-0 flex-1 pb-1">
                <button
                  type="button"
                  onClick={() => setShowInternalSteps((prev) => !prev)}
                  className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
                >
                  <ChevronRight
                    aria-hidden="true"
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
              </div>
            </div>
          )}

          {showInternalSteps &&
            hiddenItems.map((item, index) =>
              renderToolItem(item, firstHiddenIndex + index, railRowCount, {
                withRail: true,
              }),
            )}
        </div>
      )}
    </section>
  );
}
