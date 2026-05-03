import { getToolItemName, type ToolChainItem } from "./toolChainGrouping";

/**
 * Buckets used to classify a single step inside a tool chain. Ported from the
 * Rust `classify_tool_chain_step` / `summarize_tool_chain` that previously
 * lived in `crates/goose/src/acp/server.rs` so the wire stays free of any
 * `_goose/tool-chain-*` metadata.
 */
export type ToolChainStepKind =
  | "reviewing_files"
  | "running_commands"
  | "checking_resources"
  | "updating_files";

const STEP_TITLE_KEYS: Record<ToolChainStepKind, string> = {
  reviewing_files: "tool_chain.summary.reviewing_files",
  running_commands: "tool_chain.summary.running_commands",
  checking_resources: "tool_chain.summary.checking_resources",
  updating_files: "tool_chain.summary.updating_files",
};

const ACTIVE_TITLE_KEY = "tool_chain.summary.active";

const HTTP_PREFIXES = ["http://", "https://"];
const RESOURCE_TOKENS = ["fetch", "http", "url", "uri", "download"];
const UPDATE_TOKENS = [
  "edit",
  "write",
  "create",
  "update",
  "replace",
  "rename",
  "move",
  "delete",
];
const COMMAND_TOKENS = ["shell", "command", "bash", "terminal", "execute"];

function classifyStepLabel(label: string): ToolChainStepKind {
  const lower = label.toLowerCase();
  const sepIndex = lower.indexOf(" · ");
  const prefix = sepIndex === -1 ? lower : lower.slice(0, sepIndex);
  const detail = sepIndex === -1 ? "" : lower.slice(sepIndex + 3);

  if (
    HTTP_PREFIXES.some((p) => detail.startsWith(p)) ||
    RESOURCE_TOKENS.some((t) => prefix.includes(t))
  ) {
    return "checking_resources";
  }

  if (UPDATE_TOKENS.some((t) => prefix.includes(t))) {
    return "updating_files";
  }

  if (COMMAND_TOKENS.some((t) => prefix.includes(t))) {
    return "running_commands";
  }

  return "reviewing_files";
}

interface BucketCounts {
  reviewing_files: number;
  running_commands: number;
  checking_resources: number;
  updating_files: number;
}

function countBuckets(items: ToolChainItem[]): BucketCounts {
  const counts: BucketCounts = {
    reviewing_files: 0,
    running_commands: 0,
    checking_resources: 0,
    updating_files: 0,
  };
  for (const item of items) {
    counts[classifyStepLabel(getToolItemName(item))] += 1;
  }
  return counts;
}

function pickDominantBucket(counts: BucketCounts): ToolChainStepKind {
  const {
    reviewing_files,
    running_commands,
    checking_resources,
    updating_files,
  } = counts;

  if (
    updating_files > reviewing_files &&
    updating_files >= running_commands &&
    updating_files >= checking_resources
  ) {
    return "updating_files";
  }

  if (
    checking_resources > reviewing_files &&
    checking_resources >= running_commands
  ) {
    return "checking_resources";
  }

  if (running_commands > reviewing_files) {
    return "running_commands";
  }

  return "reviewing_files";
}

export interface ToolChainSummary {
  /** i18n key for the chain title (e.g. "running commands"). */
  titleKey: string;
  /** i18n key for the count suffix; consumers add count via `t(suffixKey, { count })`. */
  countKey: string;
  /** Number of steps the parent card represents. */
  count: number;
  /** Bucket the chain falls into; useful for icons/styling. */
  kind: ToolChainStepKind;
}

/**
 * Derive a localized chain summary from the per-step tool names.
 *
 * Note: when the chain is "active" (any non-completed step) callers may prefer
 * the live `tool_chain.summary.active` label over the kind-based title. This
 * function returns the deterministic kind regardless; the caller decides how
 * to render based on aggregate status.
 */
export function summarizeToolChainSteps(
  items: ToolChainItem[],
): ToolChainSummary {
  if (items.length === 0) {
    return {
      titleKey: ACTIVE_TITLE_KEY,
      countKey: "tool_chain.summary.steps",
      count: 0,
      kind: "reviewing_files",
    };
  }

  const counts = countBuckets(items);
  const kind = pickDominantBucket(counts);
  return {
    titleKey: STEP_TITLE_KEYS[kind],
    countKey: "tool_chain.summary.steps",
    count: items.length,
    kind,
  };
}

export const TOOL_CHAIN_ACTIVE_TITLE_KEY = ACTIVE_TITLE_KEY;
