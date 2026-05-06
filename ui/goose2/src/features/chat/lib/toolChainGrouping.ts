import type {
  ToolCallStatus,
  ToolRequestContent,
  ToolResponseContent,
} from "@/shared/types/messages";

/**
 * A pairing of one tool request and (optionally) its matching response,
 * preserving the order they appeared in the assistant message.
 *
 * Chains are derived from this order — see `groupAdjacentToolItems` and
 * `MessageBubble.groupContentSections` — so the server emits no chain metadata.
 */
export interface ToolChainItem {
  key: string;
  request?: ToolRequestContent;
  response?: ToolResponseContent;
}

export function getToolItemName(item: ToolChainItem): string {
  return item.request?.name || item.response?.name || "Tool result";
}

export function getToolItemStatus(item: ToolChainItem): ToolCallStatus {
  if (item.response) {
    return item.response.isError ? "failed" : "completed";
  }
  return item.request?.status ?? "completed";
}

/**
 * Aggregate status across a chain. Failure-leaning so collapsed parents don't
 * mask a failed step behind a still-pending sibling.
 */
export function getChainAggregateStatus(
  items: ToolChainItem[],
): ToolCallStatus {
  if (items.some((i) => getToolItemStatus(i) === "failed")) return "failed";
  if (items.some((i) => getToolItemStatus(i) === "stopped")) return "stopped";
  if (items.some((i) => getToolItemStatus(i) === "in_progress"))
    return "in_progress";
  if (items.some((i) => getToolItemStatus(i) === "pending")) return "pending";
  return "completed";
}

/**
 * Whether the section should render as a grouped parent card. Single-item
 * sections render inline (no parent wrapper), matching prior UX.
 */
export function shouldRenderAsGroupedChain(items: ToolChainItem[]): boolean {
  return items.length >= 2;
}
