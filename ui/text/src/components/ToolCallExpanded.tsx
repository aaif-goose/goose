import React, { useMemo } from "react";
import { Box, Text, useInput } from "ink";
import type { ToolCallContent } from "@agentclientprotocol/sdk";
import {
  formatJson,
  type ToolCallInfo,
} from "../toolcall.js";
import {
  CRANBERRY,
  TEAL,
  GOLD,
  TEXT_PRIMARY,
  TEXT_SECONDARY,
  TEXT_DIM,
} from "../colors.js";
import { SCROLL_STEP, SCROLL_FAST_MULTIPLIER } from "../constants.js";

interface Props {
  info: ToolCallInfo;
  width: number;
  height: number;
  scrollOffset: number;
  onScroll: (updater: (prev: number) => number) => void;
  onClose: () => void;
}

const STATUS_COLORS: Record<string, string> = {
  pending: TEXT_DIM,
  in_progress: GOLD,
  completed: TEAL,
  failed: CRANBERRY,
};

function wrapOrTruncate(text: string, width: number): string[] {
  const safeWidth = Math.max(width, 10);
  const out: string[] = [];
  for (const rawLine of text.split("\n")) {
    if (rawLine.length <= safeWidth) {
      out.push(rawLine);
      continue;
    }
    let remaining = rawLine;
    while (remaining.length > safeWidth) {
      out.push(remaining.slice(0, safeWidth));
      remaining = remaining.slice(safeWidth);
    }
    if (remaining.length > 0) out.push(remaining);
  }
  return out;
}

function extractContentText(content: ToolCallContent[] | undefined): string {
  if (!content || content.length === 0) return "";
  const parts: string[] = [];
  for (const item of content) {
    if (item.type === "content") {
      const block = item.content;
      if (block.type === "text" && block.text) {
        parts.push(block.text);
      } else if (block.type === "resource_link") {
        parts.push(`🔗 ${block.uri}`);
      } else if (block.type === "image") {
        parts.push(`🖼  image (${block.mimeType ?? "unknown"})`);
      } else if (block.type === "audio") {
        parts.push(`🎵 audio (${block.mimeType ?? "unknown"})`);
      } else if (block.type === "resource") {
        const res = block.resource as { uri?: string; text?: string };
        if (res.text) {
          parts.push(res.text);
        } else if (res.uri) {
          parts.push(`📎 ${res.uri}`);
        }
      }
    } else if (item.type === "diff") {
      const header = `📝 diff: ${item.path}`;
      const old = item.oldText ?? "";
      parts.push(
        [
          header,
          ...(old ? old.split("\n").map((l) => `- ${l}`) : []),
          ...item.newText.split("\n").map((l) => `+ ${l}`),
        ].join("\n"),
      );
    } else if (item.type === "terminal") {
      parts.push(`▶ terminal: ${item.terminalId}`);
    }
  }
  return parts.join("\n\n");
}

function buildBody(
  info: ToolCallInfo,
  contentWidth: number,
): React.ReactElement[] {
  const body: React.ReactElement[] = [];

  const pushLabel = (label: string, keyPrefix: string, withTopGap: boolean) => {
    if (withTopGap) {
      body.push(
        <Box key={`${keyPrefix}-gap`} height={1}>
          <Text> </Text>
        </Box>,
      );
    }
    body.push(
      <Box key={`${keyPrefix}-hdr`} height={1}>
        <Text color={TEXT_SECONDARY} bold>
          {label}
        </Text>
      </Box>,
    );
  };

  const pushText = (
    text: string,
    keyPrefix: string,
    emptyHint: string,
  ) => {
    if (!text) {
      body.push(
        <Box key={`${keyPrefix}-empty`} height={1}>
          <Text color={TEXT_DIM} italic>
            {emptyHint}
          </Text>
        </Box>,
      );
      return;
    }
    const lines = wrapOrTruncate(text, contentWidth);
    lines.forEach((l, i) => {
      body.push(
        <Box key={`${keyPrefix}-${i}`} height={1}>
          <Text color={TEXT_PRIMARY}>{l || " "}</Text>
        </Box>,
      );
    });
  };

  pushLabel(info.title, "tool", false);

  pushLabel("arguments", "in", true);
  const argsText = formatJson(info.rawInput);
  pushText(argsText, "in", "(no arguments)");

  pushLabel("result", "out", true);
  let resultText = formatJson(info.rawOutput);
  if (!resultText) {
    resultText = extractContentText(info.content);
  }
  const resultEmptyHint =
    info.status === "in_progress"
      ? "(running…)"
      : info.status === "pending"
        ? "(pending)"
        : info.status === "failed"
          ? "(failed — no output)"
          : "(no output)";
  pushText(resultText, "out", resultEmptyHint);

  return body;
}

export function ToolCallExpanded({
  info,
  width,
  height,
  scrollOffset,
  onScroll,
  onClose,
}: Props) {
  const safeWidth = Math.max(width, 20);
  const safeHeight = Math.max(height, 5);
  const contentWidth = Math.max(safeWidth - 4, 10);

  const allLines = useMemo(
    () => buildBody(info, contentWidth),
    [info, contentWidth],
  );

  useInput((ch, key) => {
    if (key.escape || ch === " ") {
      onClose();
      return;
    }
    if (key.upArrow || key.downArrow) {
      const step = key.meta
        ? SCROLL_STEP * SCROLL_FAST_MULTIPLIER
        : SCROLL_STEP;
      if (key.upArrow) {
        onScroll((prev) => prev + step);
      } else {
        onScroll((prev) => Math.max(prev - step, 0));
      }
    }
  });

  const headerH = 2;
  const footerH = 2;
  const bodyHeight = Math.max(safeHeight - headerH - footerH, 1);

  const total = allLines.length;
  const overflows = total > bodyHeight;
  const contentHeight = overflows ? Math.max(bodyHeight - 2, 1) : bodyHeight;

  const maxEnd = total;
  const minEnd = Math.min(contentHeight, total);
  const endIdx = Math.max(minEnd, Math.min(maxEnd - scrollOffset, maxEnd));
  const startIdx = Math.max(0, endIdx - contentHeight);
  const visible = allLines.slice(startIdx, endIdx);
  const padCount = contentHeight - visible.length;

  const elements: React.ReactElement[] = [];
  if (overflows) {
    const above = startIdx;
    elements.push(
      <Box key="exp-up" width={safeWidth} height={1} justifyContent="center">
        {above > 0 ? (
          <Text color={TEXT_DIM}>▲ {above} more (↑)</Text>
        ) : (
          <Text> </Text>
        )}
      </Box>,
    );
  }
  for (let i = 0; i < padCount; i++) {
    elements.push(
      <Box key={`exp-pad-${i}`} width={safeWidth} height={1}>
        <Text> </Text>
      </Box>,
    );
  }
  elements.push(...visible);
  if (overflows) {
    const below = total - endIdx;
    elements.push(
      <Box key="exp-dn" width={safeWidth} height={1} justifyContent="center">
        {below > 0 ? (
          <Text color={TEXT_DIM}>▼ {below} more (↓)</Text>
        ) : (
          <Text> </Text>
        )}
      </Box>,
    );
  }

  const statusColor = STATUS_COLORS[info.status] ?? TEXT_DIM;

  return (
    <Box
      flexDirection="column"
      width={safeWidth}
      height={safeHeight}
      borderStyle="round"
      borderColor={GOLD}
      paddingX={1}
    >
      <Box width={contentWidth} height={1}>
        <Text color={statusColor}>●</Text>
        <Text color={TEXT_DIM}> {info.status}</Text>
        <Box flexGrow={1} />
        <Text color={TEXT_DIM} italic>
          space/esc to close
        </Text>
      </Box>
      <Box flexDirection="column" width={contentWidth} height={bodyHeight}>
        {elements}
      </Box>
      <Box width={contentWidth} height={1}>
        <Text color={TEXT_DIM}>↑↓ scroll · ⌥↑↓ fast</Text>
      </Box>
    </Box>
  );
}
