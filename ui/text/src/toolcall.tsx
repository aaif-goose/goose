import React from "react";
import { Box, Text } from "ink";
import type {
  ToolCallContent,
  ToolCallStatus,
  ToolKind,
} from "@agentclientprotocol/sdk";
import { CRANBERRY, TEAL, GOLD, TEXT_SECONDARY, TEXT_DIM } from "./colors.js";

export interface ToolCallInfo {
  toolCallId: string;
  title: string;
  status: ToolCallStatus;
  kind?: ToolKind;
  rawInput?: unknown;
  rawOutput?: unknown;
  content?: ToolCallContent[];
  locations?: Array<{ path: string; line?: number | null }>;
}

const CEDAR = "#6B5344";

const KIND_ICONS: Record<string, string> = {
  read: "📖",
  edit: "✏️",
  delete: "🗑",
  move: "📦",
  search: "🔍",
  execute: "▶",
  think: "💭",
  fetch: "🌐",
  switch_mode: "🔀",
  other: "⚙",
};

const STATUS_INDICATORS: Record<string, { icon: string; color: string }> = {
  pending: { icon: "○", color: TEXT_DIM },
  in_progress: { icon: "◑", color: GOLD },
  completed: { icon: "●", color: TEAL },
  failed: { icon: "✗", color: CRANBERRY },
};

function truncateLine(line: string, maxWidth: number): string {
  const safeMaxWidth = Math.max(maxWidth, 1);
  if (line.length <= safeMaxWidth) return line;
  return safeMaxWidth > 1
    ? line.slice(0, safeMaxWidth - 1) + "…"
    : line.slice(0, safeMaxWidth);
}

export function formatJson(value: unknown): string {
  if (value === undefined || value === null) return "";
  if (typeof value === "string") {
    // If it looks like JSON, try to parse and re-format; otherwise return as-is.
    const trimmed = value.trim();
    if (trimmed.startsWith("{") || trimmed.startsWith("[")) {
      try {
        return JSON.stringify(JSON.parse(trimmed), null, 2);
      } catch {
        return value;
      }
    }
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/**
 * Render a tool call as a single-line boxed summary.
 *
 * The box always has the same content and height as before; when `selected`
 * is true we swap the border color and show a hint that space will expand it.
 */
export function renderToolCallLines(
  info: ToolCallInfo,
  width: number,
  selected: boolean,
): React.ReactElement[] {
  const kindIcon = KIND_ICONS[info.kind ?? "other"] ?? "⚙";
  const statusInfo =
    STATUS_INDICATORS[info.status] ?? STATUS_INDICATORS.pending!;

  const borderColor = selected
    ? GOLD
    : info.status === "failed"
      ? CRANBERRY
      : CEDAR;
  const dimBorder = !selected && info.status !== "failed";

  const safeWidth = Math.max(width, 10);
  const innerWidth = Math.max(safeWidth - 4, 6);

  const k = info.toolCallId;
  const lines: React.ReactElement[] = [];

  const hRule = "─".repeat(Math.max(safeWidth - 2, 0));
  lines.push(
    <Box key={`${k}-t`} width={safeWidth} height={1}>
      <Text color={borderColor} dimColor={dimBorder}>
        ╭{hRule}╮
      </Text>
    </Box>,
  );

  const statusIcon = statusInfo.icon;
  const runningText = info.status === "in_progress" ? " running…" : "";
  const hintText = selected ? "space to expand" : "";
  const fixedLen = 4 + runningText.length + hintText.length;
  const titleMax = Math.max(innerWidth - fixedLen, 4);
  const title = truncateLine(info.title, titleMax);

  lines.push(
    <Box key={`${k}-h`} width={safeWidth} height={1}>
      <Text color={borderColor} dimColor={dimBorder}>
        │{" "}
      </Text>
      <Box width={innerWidth} height={1}>
        <Text color={statusInfo.color}>{statusIcon}</Text>
        <Text> {kindIcon} </Text>
        <Text wrap="truncate-end" color={TEXT_SECONDARY} bold>
          {title}
        </Text>
        {runningText ? (
          <Text color={TEXT_DIM} italic>
            {runningText}
          </Text>
        ) : null}
        <Box flexGrow={1} />
        {hintText ? (
          <Text color={GOLD} italic>
            {hintText}
          </Text>
        ) : null}
      </Box>
      <Text color={borderColor} dimColor={dimBorder}>
        {" "}
        │
      </Text>
    </Box>,
  );

  lines.push(
    <Box key={`${k}-b`} width={safeWidth} height={1}>
      <Text color={borderColor} dimColor={dimBorder}>
        ╰{hRule}╯
      </Text>
    </Box>,
  );

  return lines;
}

/**
 * Height in lines of the rendered single-line tool-call box.
 * Kept in sync with `renderToolCallLines`.
 */
export const TOOL_CALL_BOX_HEIGHT = 3;
