import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronRight, CircleIcon, ClockIcon } from "lucide-react";
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

const STEP_BULLET_ICON: Record<
  Exclude<ToolCallStatus, "failed" | "stopped">,
  LucideIcon
> = {
  pending: CircleIcon,
  in_progress: ClockIcon,
  completed: Check,
};

const STEP_BULLET_CLASS: Record<
  Exclude<ToolCallStatus, "failed" | "stopped">,
  string
> = {
  pending: "text-muted-foreground/70",
  in_progress: "text-muted-foreground animate-pulse",
  completed: "text-muted-foreground",
};

function ChainStepBullet({ status }: { status: ToolCallStatus }) {
  if (status === "failed") {
    return (
      <span aria-hidden className="size-1.5 shrink-0 rounded-full bg-red-600" />
    );
  }
  if (status === "stopped") {
    return (
      <span
        aria-hidden
        className="size-1.5 shrink-0 rounded-full bg-orange-600"
      />
    );
  }
  const Icon = STEP_BULLET_ICON[status];
  return (
    <Icon className={cn("size-3.5 shrink-0", STEP_BULLET_CLASS[status])} />
  );
}

function ChainStepRail({
  status,
  isLast = false,
  lineTailVisible = true,
}: {
  status: ToolCallStatus;
  /** Last row in the expanded chain: hides the spine stub below the bullet until `lineTailVisible`. */
  isLast?: boolean;
  lineTailVisible?: boolean;
}) {
  return (
    <div
      aria-hidden="true"
      className="relative flex w-4 shrink-0 justify-center self-stretch"
    >
      {isLast && (
        <div
          className={cn(
            "pointer-events-none absolute top-5 bottom-0 left-1/2 z-11 w-2.5 -translate-x-1/2 bg-background transition-opacity duration-150",
            lineTailVisible ? "opacity-0" : "opacity-100",
          )}
        />
      )}
      <div className="relative z-10 mt-1 flex h-4 w-4 items-center justify-center rounded-full bg-background ring-2 ring-background">
        <ChainStepBullet status={status} />
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
  const { primaryItems, hiddenItems } = partitionToolSteps(toolItems);
  const grouped = shouldRenderAsGroupedChain(toolItems);
  const aggregateStatus = getChainAggregateStatus(toolItems);
  const summary = summarizeToolChainSteps(primaryItems);
  const isActiveChain =
    aggregateStatus === "in_progress" || aggregateStatus === "pending";
  // Chains that mount as already-complete (history replay) start collapsed;
  // live chains mount mid-execution, stay open while running, and auto-collapse
  // once they finish so the chat keeps moving forward.
  const [chainExpanded, setChainExpanded] = useState(() => isActiveChain);
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());
  const wasActiveChainRef = useRef(isActiveChain);
  useEffect(() => {
    if (wasActiveChainRef.current && !isActiveChain) {
      setChainExpanded(false);
      setExpandedKeys(new Set());
      setShowInternalSteps(false);
    }
    wasActiveChainRef.current = isActiveChain;
  }, [isActiveChain]);

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
    options: { withRail: boolean; isLastInChain?: boolean },
  ) => {
    const name = getToolItemName(item);
    const status = getToolItemStatus(item);
    const { request, response } = item;
    const isOpen = expandedKeys.has(item.key);

    if (!options.withRail) {
      return (
        <div
          key={item.key}
          data-role="tool-single"
          className="flex max-w-full items-start gap-2.5"
        >
          <button
            type="button"
            onClick={() => handleOpenChange(item.key, !isOpen)}
            aria-expanded={isOpen}
            aria-label={name}
            className="flex w-4 shrink-0 justify-center pt-1"
          >
            <span className="flex size-4 items-center justify-center">
              <ChevronRight
                aria-hidden="true"
                className={cn(
                  "size-3.5 shrink-0 text-muted-foreground transition-transform",
                  isOpen && "rotate-90",
                )}
              />
            </span>
          </button>
          <div className="min-w-0 flex-1">
            <ToolCallAdapter
              name={name}
              arguments={request?.arguments ?? {}}
              status={status}
              locations={request?.locations}
              result={response?.result}
              structuredContent={response?.structuredContent}
              isError={response?.isError}
              startedAt={request?.startedAt}
              open={isOpen}
              onOpenChange={(open) => handleOpenChange(item.key, open)}
              showChevron={false}
            />
          </div>
        </div>
      );
    }

    return (
      <div
        key={item.key}
        data-role="tool-chain-step"
        className="relative z-1 flex max-w-full items-stretch gap-2.5"
      >
        <ChainStepRail
          status={status}
          isLast={options.isLastInChain}
          lineTailVisible={isOpen}
        />
        <div className="min-w-0 flex-1 pb-1">
          <ToolCallAdapter
            name={name}
            arguments={request?.arguments ?? {}}
            status={status}
            locations={request?.locations}
            result={response?.result}
            structuredContent={response?.structuredContent}
            isError={response?.isError}
            startedAt={request?.startedAt}
            open={isOpen}
            onOpenChange={(open) => handleOpenChange(item.key, open)}
            showStatusBadge={false}
            fitWidth
          />
        </div>
      </div>
    );
  };

  if (!grouped) {
    return (
      <div className="my-3 flex w-full min-w-0 max-w-full flex-col gap-3">
        {primaryItems.map((item) => renderToolItem(item, { withRail: false }))}
      </div>
    );
  }

  // Prefer the server-generated LLM chain summary (anchored on the first tool
  // request of the chain) over the deterministic bucket phrase. The summary is
  // attached after every step in the chain has completed, so it's only
  // available for finished chains; while the chain is still active, fall back
  // to the deterministic phrase.
  const firstChainSummary = toolItems.find((item) => item.request?.chainSummary)
    ?.request?.chainSummary;
  const labelText =
    !isActiveChain && firstChainSummary
      ? firstChainSummary.summary
      : isActiveChain
        ? t("tool_chain.summary.active")
        : t(summary.titleKey);
  const headerText = isActiveChain
    ? t("tool_chain.title.active", { count: toolItems.length })
    : t("tool_chain.title.labeled", {
        label: labelText,
        count: toolItems.length,
      });

  const hasHiddenDisclosure = hiddenItems.length > 0;

  return (
    <section
      className="my-3 flex w-full min-w-0 max-w-full flex-col gap-0"
      data-role="tool-chain-card"
      data-status={aggregateStatus}
    >
      <button
        type="button"
        onClick={() => {
          if (chainExpanded) {
            setExpandedKeys(new Set());
            setShowInternalSteps(false);
          }
          setChainExpanded((prev) => !prev);
        }}
        aria-expanded={chainExpanded}
        className="flex w-full max-w-full items-center gap-2.5 pb-1 text-left text-sm font-medium text-foreground"
      >
        <span
          aria-hidden="true"
          className="flex size-4 shrink-0 items-center justify-center"
        >
          <ChevronRight
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground transition-transform",
              chainExpanded && "rotate-90",
            )}
          />
        </span>
        <span className="min-w-0 flex-1 truncate">{headerText}</span>
      </button>

      {chainExpanded && (
        <div className="relative flex flex-col gap-1 pt-1.5">
          <div
            aria-hidden="true"
            className="pointer-events-none absolute top-0 bottom-0 left-2 z-0 w-px -translate-x-1/2 bg-border"
          />
          {primaryItems.map((item, index) => {
            const isLastPrimary = index === primaryItems.length - 1;
            const isLastInChain =
              isLastPrimary &&
              !hasHiddenDisclosure &&
              !(showInternalSteps && hiddenItems.length > 0);
            return renderToolItem(item, {
              withRail: true,
              isLastInChain,
            });
          })}

          {hasHiddenDisclosure && (
            <div
              data-role="tool-chain-internal-disclosure"
              className="relative z-1 flex max-w-full items-stretch gap-2.5"
            >
              <ChainStepRail
                status="completed"
                isLast={!showInternalSteps}
                lineTailVisible={showInternalSteps}
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
              renderToolItem(item, {
                withRail: true,
                isLastInChain: index === hiddenItems.length - 1,
              }),
            )}
        </div>
      )}
    </section>
  );
}
