import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { ToolCallAdapter } from "./ToolCallAdapter";
import type {
  ToolRequestContent,
  ToolResponseContent,
} from "@/shared/types/messages";

export interface ToolChainItem {
  key: string;
  request?: ToolRequestContent;
  response?: ToolResponseContent;
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

function getToolItemName(item: ToolChainItem): string {
  return item.request?.name || item.response?.name || "Tool result";
}

function getToolItemStatus(item: ToolChainItem) {
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

export function ToolChainCards({ toolItems }: { toolItems: ToolChainItem[] }) {
  const [showInternalSteps, setShowInternalSteps] = useState(false);
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());
  const { primaryItems, hiddenItems } = partitionToolSteps(toolItems);

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
        result={response?.result}
        isError={response?.isError}
        startedAt={request?.startedAt}
        open={expandedKeys.has(item.key)}
        onOpenChange={(open) => handleOpenChange(item.key, open)}
      />
    );
  };

  return (
    <div className="my-1 flex flex-col items-start gap-3">
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
              ? `Hide internal steps (${hiddenItems.length})`
              : `Show internal steps (${hiddenItems.length})`}
          </button>

          {showInternalSteps && hiddenItems.map((item) => renderToolItem(item))}
        </div>
      )}
    </div>
  );
}
