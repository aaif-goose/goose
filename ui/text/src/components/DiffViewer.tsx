import React, { useEffect, useMemo, useState } from "react";
import { Box, Text, useInput } from "ink";
import {
  TEXT_DIM,
  TEXT_PRIMARY,
  GOLD,
  TEAL,
  CRANBERRY,
  TEXT_SECONDARY,
} from "../colors.js";
import { SCROLL_FAST_MULTIPLIER } from "../constants.js";

const PAD_X = 2;
const PAD_Y = 1;
const HEADER_LINES = 1;
const FOOTER_LINES = 1;

type LineKind = "add" | "remove" | "hunk" | "meta" | "context";

function classifyLine(line: string): LineKind {
  if (line.startsWith("+++") || line.startsWith("---")) return "meta";
  if (
    line.startsWith("diff ") ||
    line.startsWith("index ") ||
    line.startsWith("new file") ||
    line.startsWith("deleted file") ||
    line.startsWith("rename ") ||
    line.startsWith("similarity ") ||
    line.startsWith("Binary ")
  ) {
    return "meta";
  }
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+")) return "add";
  if (line.startsWith("-")) return "remove";
  return "context";
}

function padLine(line: string, width: number): string {
  if (line.length >= width) return line.slice(0, width);
  return line + " ".repeat(width - line.length);
}

interface Props {
  content: string;
  truncated: boolean;
  width: number;
  height: number;
  onClose: () => void;
}

export function DiffViewer({
  content,
  truncated,
  width,
  height,
  onClose,
}: Props) {
  const lines = useMemo(() => {
    const split = content.split("\n");
    if (split.length > 0 && split[split.length - 1] === "") split.pop();
    return split;
  }, [content]);

  const innerWidth = Math.max(width - PAD_X * 2, 10);
  const innerHeight = Math.max(height - PAD_Y * 2, 3);
  const viewportHeight = Math.max(
    innerHeight - HEADER_LINES - FOOTER_LINES,
    1,
  );
  const maxScroll = Math.max(lines.length - viewportHeight, 0);

  const [scroll, setScroll] = useState(0);

  useEffect(() => {
    setScroll((prev) => Math.min(prev, maxScroll));
  }, [maxScroll]);

  useInput((ch, key) => {
    if (ch === "q" || ch === "Q" || key.escape) {
      onClose();
      return;
    }
    if (key.ctrl && (ch === "c" || ch === "C")) {
      onClose();
      return;
    }

    if (key.downArrow || ch === "j") {
      const step = key.meta ? SCROLL_FAST_MULTIPLIER : 1;
      setScroll((s) => Math.min(s + step, maxScroll));
      return;
    }
    if (key.upArrow || ch === "k") {
      const step = key.meta ? SCROLL_FAST_MULTIPLIER : 1;
      setScroll((s) => Math.max(s - step, 0));
      return;
    }
    if (key.pageDown || ch === " " || (key.ctrl && ch === "d")) {
      setScroll((s) => Math.min(s + viewportHeight, maxScroll));
      return;
    }
    if (key.pageUp || ch === "b" || (key.ctrl && ch === "u")) {
      setScroll((s) => Math.max(s - viewportHeight, 0));
      return;
    }
    if (ch === "g") {
      setScroll(0);
      return;
    }
    if (ch === "G") {
      setScroll(maxScroll);
      return;
    }
  });

  const visible = lines.slice(scroll, scroll + viewportHeight);

  const atEnd = scroll >= maxScroll;
  const atStart = scroll === 0;
  const position = maxScroll === 0
    ? "ALL"
    : atEnd
    ? "END"
    : `${Math.round((scroll / maxScroll) * 100)}%`;

  return (
    <Box
      flexDirection="column"
      width={width}
      height={height}
      paddingX={PAD_X}
      paddingY={PAD_Y}
    >
      <Box width={innerWidth} justifyContent="space-between" flexShrink={0}>
        <Text color={TEXT_PRIMARY} bold>
          git diff{truncated ? " (truncated)" : ""}
        </Text>
        <Text color={TEXT_DIM}>
          {atStart ? "" : "↑ "}lines {scroll + 1}–
          {Math.min(scroll + viewportHeight, lines.length)} / {lines.length}
          {" "}[{position}]
        </Text>
      </Box>
      <Box flexDirection="column" width={innerWidth} height={viewportHeight}>
        {visible.map((line, i) => {
          const kind = classifyLine(line);
          const padded = padLine(line, innerWidth);
          switch (kind) {
            case "add":
              return (
                <Text
                  key={i}
                  wrap="truncate-end"
                  color={TEXT_PRIMARY}
                  backgroundColor={TEAL}
                >
                  {padded}
                </Text>
              );
            case "remove":
              return (
                <Text
                  key={i}
                  wrap="truncate-end"
                  color={TEXT_PRIMARY}
                  backgroundColor={CRANBERRY}
                >
                  {padded}
                </Text>
              );
            case "hunk":
              return (
                <Text key={i} wrap="truncate-end" color={GOLD} bold>
                  {padded}
                </Text>
              );
            case "meta":
              return (
                <Text key={i} wrap="truncate-end" color={TEXT_SECONDARY} bold>
                  {padded}
                </Text>
              );
            default:
              return (
                <Text key={i} wrap="truncate-end" color={TEXT_PRIMARY}>
                  {padded}
                </Text>
              );
          }
        })}
      </Box>
      <Box width={innerWidth} flexShrink={0}>
        <Text color={GOLD}>q</Text>
        <Text color={TEXT_DIM}> close · </Text>
        <Text color={GOLD}>↑↓</Text>
        <Text color={TEXT_DIM}>/</Text>
        <Text color={GOLD}>j k</Text>
        <Text color={TEXT_DIM}> scroll · </Text>
        <Text color={GOLD}>space</Text>
        <Text color={TEXT_DIM}>/</Text>
        <Text color={GOLD}>b</Text>
        <Text color={TEXT_DIM}> page · </Text>
        <Text color={GOLD}>g</Text>
        <Text color={TEXT_DIM}>/</Text>
        <Text color={GOLD}>G</Text>
        <Text color={TEXT_DIM}> top/bottom</Text>
      </Box>
    </Box>
  );
}
