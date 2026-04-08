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
  if (line.length <= maxWidth) return line;
  return maxWidth > 1 ? line.slice(0, maxWidth - 1) + "…" : line.slice(0, maxWidth);
}

function formatJsonLines(value: unknown, maxWidth: number): string[] {
  if (value === undefined || value === null) return [];
  let raw: string;
  if (typeof value === "string") {
    raw = value;
  } else {
    try {
      raw = JSON.stringify(value, null, 2);
    } catch {
      raw = String(value);
    }
  }
  return raw.split("\n").map((line) => truncateLine(line, maxWidth));
}

function extractTextLines(content: ToolCallContent[], maxWidth: number): string[] {
  const lines: string[] = [];
  for (const item of content) {
    if (item.type === "content" && item.content) {
      const block = item.content as any;
      if (block.type === "text" && block.text) {
        for (const line of block.text.split("\n")) {
          lines.push(truncateLine(line, maxWidth));
        }
      }
    } else if (item.type === "diff") {
      const diff = item as any;
      lines.push(truncateLine(`diff: ${diff.path || "unknown"}`, maxWidth));
    } else if (item.type === "terminal") {
      const term = item as any;
      lines.push(truncateLine(`terminal: ${term.terminalId || "unknown"}`, maxWidth));
    }
  }
  return lines;
}

export function renderToolCallLines(
  info: ToolCallInfo,
  width: number,
  expanded: boolean,
  showTabHint: boolean,
): React.ReactElement[] {
  const kindIcon = KIND_ICONS[info.kind ?? "other"] ?? "⚙";
  const statusInfo = STATUS_INDICATORS[info.status] ?? STATUS_INDICATORS.pending!;
  const borderColor = info.status === "failed" ? CRANBERRY : CEDAR;
  const dimBorder = info.status !== "failed";

  const innerWidth = Math.max(width - 4, 10);
  const indentedWidth = Math.max(innerWidth - 2, 8);

  const lines: React.ReactElement[] = [];
  const k = info.toolCallId;

  const hRule = "─".repeat(Math.max(width - 2, 0));
  lines.push(
    <Box key={`${k}-t`} width={width} height={1}>
      <Text color={borderColor} dimColor={dimBorder}>╭{hRule}╮</Text>
    </Box>,
  );

  const row = (key: string, content: React.ReactNode) => {
    lines.push(
      <Box key={key} width={width} height={1}>
        <Text color={borderColor} dimColor={dimBorder}>│ </Text>
        <Box width={innerWidth} height={1}>
          {content}
        </Box>
        <Text color={borderColor} dimColor={dimBorder}> │</Text>
      </Box>,
    );
  };

  const statusIcon = statusInfo.icon;
  const runningText = info.status === "in_progress" ? " running…" : "";
  const tabHintText = showTabHint && !expanded ? "tab ↔" : "";
  const fixedLen = 4 + runningText.length + tabHintText.length; // icon+space+kind+space + suffix + hint
  const titleMax = Math.max(innerWidth - fixedLen, 4);
  const title = truncateLine(info.title, titleMax);

  row(`${k}-h`, (
    <>
      <Text color={statusInfo.color}>{statusIcon}</Text>
      <Text> {kindIcon} </Text>
      <Text wrap="truncate-end" color={TEXT_SECONDARY} bold>{title}</Text>
      {runningText ? <Text color={TEXT_DIM} italic>{runningText}</Text> : null}
      <Box flexGrow={1} />
      {tabHintText ? <Text color={TEXT_DIM} italic>{tabHintText}</Text> : null}
    </>
  ));

  if (expanded) {
    if (info.locations) {
      for (let i = 0; i < info.locations.length; i++) {
        const loc = info.locations[i]!;
        const t = truncateLine(`📁 ${loc.path}${loc.line ? `:${loc.line}` : ""}`, innerWidth);
        row(`${k}-l${i}`, <Text wrap="truncate-end" color={TEXT_DIM}>{t}</Text>);
      }
    }

    const section = (label: string, sLines: string[]) => {
      if (sLines.length === 0) return;
      row(`${k}-${label}H`, <Text color={TEXT_DIM}>▸ {label}:</Text>);
      for (let i = 0; i < sLines.length; i++) {
        row(`${k}-${label}${i}`, (
          <Text wrap="truncate-end" color={TEXT_DIM}>{"  "}{sLines[i]}</Text>
        ));
      }
    };

    if (info.rawInput !== undefined && info.rawInput !== null) {
      section("in", formatJsonLines(info.rawInput, indentedWidth));
    }
    if (info.rawOutput !== undefined && info.rawOutput !== null) {
      section("out", formatJsonLines(info.rawOutput, indentedWidth));
    }
    if (info.content && info.content.length > 0) {
      section("ct", extractTextLines(info.content, indentedWidth));
    }
  }

  lines.push(
    <Box key={`${k}-b`} width={width} height={1}>
      <Text color={borderColor} dimColor={dimBorder}>╰{hRule}╯</Text>
    </Box>,
  );

  return lines;
}
