import React from "react";
import { Box, Text } from "ink";
import { renderMarkdown } from "../markdown.js";
import { renderToolCallLines } from "../toolcall.js";
import type { ToolCallInfo } from "../toolcall.js";
import type { ResponseItem } from "../types.js";
import { CRANBERRY, TEXT_DIM, GOLD } from "../colors.js";
import { Spinner } from "./Spinner.js";

export function emptyLine(key: string, width: number): React.ReactElement {
  return <Box key={key} width={width} height={1}><Text> </Text></Box>;
}

export function renderUserPrompt(
  userText: string, 
  width: number,
  turnId: string,
  collapsedUserPrompt: (text: string, width: number) => React.ReactElement
): React.ReactElement[] {
  const constrainedWidth = Math.max(width - 4, 10);
  return [
    emptyLine(`u-gap-${turnId}`, width),
    <Box key={`u-prompt-${turnId}`} width={width} height={1}>
      <Text color={CRANBERRY} bold>{"❯ "}</Text>
      <Box width={constrainedWidth}>
        {collapsedUserPrompt(userText, constrainedWidth)}
      </Box>
    </Box>,
  ];
}

export function renderToolCallItem(
  item: ResponseItem & { itemType: "tool_call" },
  index: number,
  width: number,
  toolCallsExpanded: boolean,
  isFirst: boolean,
  hasToolCalls: boolean
): React.ReactElement[] {
  const info: ToolCallInfo = {
    toolCallId: item.toolCallId,
    title: item.title,
    status: item.status ?? "pending",
    kind: item.kind,
    rawInput: item.rawInput,
    rawOutput: item.rawOutput,
    content: item.content,
    locations: item.locations,
  };
  
  return [
    emptyLine(`tc-gap-${index}`, width),
    ...renderToolCallLines(info, width, toolCallsExpanded, isFirst && hasToolCalls),
  ];
}

export function renderErrorItem(
  item: ResponseItem & { itemType: "error" },
  index: number,
  width: number
): React.ReactElement[] {
  const lines: React.ReactElement[] = [
    emptyLine(`err-gap-${index}`, width),
    <Box key={`err-box-${index}`} width={width} height={1}>
      <Text color={CRANBERRY} bold>{"⚠ Error: "}</Text>
    </Box>,
  ];

  const errorLines = item.message.split("\n");
  errorLines.forEach((line, j) => {
    lines.push(
      <Box key={`err-${index}-${j}`} width={width} height={1}>
        <Box width={width}>
          <Text color={CRANBERRY} wrap="truncate">{line}</Text>
        </Box>
      </Box>
    );
  });

  return lines;
}

export function renderContentItem(
  item: ResponseItem & { itemType: "content_chunk" },
  index: number,
  width: number
): React.ReactElement[] {
  if (item.content.type !== "text" || !item.content.text) {
    return [];
  }

  const constrainedWidth = Math.max(width - 2, 10);
  const mdLines = renderMarkdown(item.content.text, constrainedWidth);
  const lines: React.ReactElement[] = [emptyLine(`md-gap-${index}`, width)];
  
  mdLines.forEach((mdLine, j) => {
    lines.push(
      <Box key={`md-${index}-${j}`} width={width} height={1}>
        <Box width={constrainedWidth}>
          <Text wrap="truncate">{mdLine}</Text>
        </Box>
      </Box>
    );
  });

  return lines;
}

export function renderLoadingIndicator(
  status: string,
  spinIdx: number,
  width: number
): React.ReactElement[] {
  return [
    emptyLine("ld-gap", width),
    <Box key="ld" width={width} height={1}>
      <Spinner idx={spinIdx} />
      <Text color={TEXT_DIM} italic> {status}</Text>
    </Box>,
  ];
}

export function renderQueuedMessages(
  queuedMessages: string[],
  width: number
): React.ReactElement[] {
  const messageWidth = Math.max(width - 20, 10);
  return queuedMessages.map((message, i) => (
    <Box key={`q-${i}`} width={width} height={1}>
      <Text color={TEXT_DIM}>{"❯ "}</Text>
      <Box width={messageWidth}>
        <Text wrap="truncate-end" color={TEXT_DIM}>{message}</Text>
      </Box>
      <Text color={GOLD} dimColor> (queued)</Text>
    </Box>
  ));
}
