import {
  getElicitationContent,
  getTextAndImageContent,
  getToolRequests,
  type Message,
  type MessageContent,
  type ToolRequestMessageContent,
} from '../types/message';
import { isFinalOutputToolRequest } from './FinalOutputLine';
import { getMcpAppMetadata } from './ToolCallWithResponse';
import type { MessageRowContext, ToolRenderState } from './messageRowContext';

export interface IndexedMessage {
  message: Message;
  index: number;
}

export interface ActivityMessage extends IndexedMessage {
  key: number;
}

interface ActivityTool {
  request: ToolRequestMessageContent;
  state: ToolRenderState;
}

export type TurnStatus = 'running' | 'completed' | 'failed' | 'incomplete' | 'waiting';

export interface TurnActivity {
  start: number;
  end: number;
  active: boolean;
  toolCount: number;
  status: TurnStatus;
  runningTool: string | undefined;
  lastTool: string | undefined;
  steps: ActivityMessage[];
  inline: Map<number, Message>;
  footerIndex: number | undefined;
}

function isTurnBoundary(message: Message, isUserMessage: (message: Message) => boolean) {
  return (
    isUserMessage(message) &&
    !message.metadata.steer &&
    !message.content.some((content) => content.type === 'toolResponse') &&
    message.content.some((content) => content.type === 'text' || content.type === 'image')
  );
}

function isAnswerContent(content: MessageContent) {
  return content.type === 'image' || (content.type === 'text' && content.text.trim() !== '');
}

function isThinking(content: MessageContent) {
  return content.type === 'thinking' && Boolean(content.thinking);
}

function lastToolRequestIndex(message: Message) {
  let index = message.content.length - 1;
  while (index >= 0) {
    const content = message.content[index];
    if (content.type === 'toolRequest' && !isFinalOutputToolRequest(content)) break;
    index--;
  }
  return index;
}

function toolLabel({ request }: ActivityTool) {
  const call = request.toolCall as { status?: string; value?: { name?: string }; name?: string };
  const name = call.status === 'success' ? call.value?.name : call.name;
  return name?.split('__').at(-1)?.replaceAll('_', ' ');
}

function toolFailed({ request, state }: ActivityTool) {
  const result = state.response?.toolResult as
    { status?: string; isError?: boolean; value?: { isError?: boolean } } | undefined;
  return (
    request.toolCall.status === 'error' ||
    result?.status === 'error' ||
    result?.isError === true ||
    result?.value?.isError === true
  );
}

function describeTurn(
  rows: IndexedMessage[],
  rowContexts: MessageRowContext[],
  active: boolean,
  showThinking: boolean
): TurnActivity {
  const tools = new Map<string, ActivityTool>();
  let lastTextIndex: number | undefined;
  let lastToolIndex: number | undefined;
  let lastAssistantIndex: number | undefined;
  let awaitingConfirmation = false;
  let awaitingElicitation = false;

  for (const { message, index } of rows) {
    if (message.role !== 'assistant') continue;

    const rowContext = rowContexts[index];
    awaitingConfirmation ||= rowContext.hasPendingConfirmation;
    const elicitation = getElicitationContent(message);
    if (elicitation?.data.actionType === 'elicitation') {
      const data = elicitation.data as typeof elicitation.data & {
        isSubmitted?: boolean;
        isCancelled?: boolean;
      };
      awaitingElicitation ||= !data.isSubmitted && !data.isCancelled;
    }

    if (
      message.content.some(
        (content) =>
          isAnswerContent(content) || isThinking(content) || content.type === 'toolRequest'
      ) ||
      message.metadata.outputTokenLimitReached
    ) {
      lastAssistantIndex = index;
    }
    getToolRequests(message).forEach((request, toolIndex) => {
      if (!isFinalOutputToolRequest(request)) {
        tools.set(request.id, { request, state: rowContext.toolStates[toolIndex] });
        lastToolIndex = index;
      }
    });
    if (!message.metadata.fallbackContent) {
      const lastRequest = lastToolRequestIndex(message);
      const content = lastRequest < 0 ? message.content : message.content.slice(lastRequest + 1);
      const { textContent } = getTextAndImageContent({
        ...message,
        content: content.filter((block) => block.type === 'text'),
      });
      if (textContent) lastTextIndex = index;
    }
  }

  const calls = [...tools.values()];
  const hasActivity =
    calls.length > 0 ||
    (showThinking &&
      rows.some(({ message }) => message.role === 'assistant' && message.content.some(isThinking)));
  const unanswered = calls.filter((tool) => !tool.state.response && !toolFailed(tool));
  const waiting =
    calls.some((tool) => tool.state.isPending) ||
    (active && (awaitingElicitation || awaitingConfirmation));
  const status = waiting
    ? 'waiting'
    : active
      ? 'running'
      : calls.some(toolFailed)
        ? 'failed'
        : unanswered.length > 0
          ? 'incomplete'
          : 'completed';
  const finalTextIndex =
    lastTextIndex !== undefined && lastTextIndex >= (lastToolIndex ?? -1)
      ? lastTextIndex
      : undefined;
  let footerIndex = finalTextIndex;
  const steps: ActivityMessage[] = [];
  const inline = new Map<number, Message>();

  for (const { message, index } of rows) {
    if (message.role !== 'assistant') continue;
    const inlineContent: MessageContent[] = [];
    let stepContent: MessageContent[] = [];
    let stepKey = 0;
    const flushStep = () => {
      if (stepContent.some((content) => content.type !== 'text' || content.text.trim())) {
        steps.push({
          message: {
            ...message,
            content: stepContent,
            metadata: { ...message.metadata, outputTokenLimitReached: false },
          },
          index,
          key: stepKey,
        });
      }
      stepContent = [];
    };
    const lastToolContentIndex = lastToolRequestIndex(message);
    message.content.forEach((content, contentIndex) => {
      if (
        content.type === 'redactedThinking' ||
        (content.type === 'thinking' && (!showThinking || !isThinking(content)))
      )
        return;

      const tool = content.type === 'toolRequest' ? tools.get(content.id) : undefined;
      const hasMcpApp = tool && getMcpAppMetadata(tool.state.response);
      const finalContent =
        index >= (lastToolIndex ?? -1) &&
        contentIndex > lastToolContentIndex &&
        (content.type === 'image' || (finalTextIndex !== undefined && index >= finalTextIndex));
      const group =
        (tool && !tool.state.isPending) ||
        isThinking(content) ||
        (hasActivity &&
          !finalContent &&
          !message.metadata.fallbackContent &&
          (content.type === 'text' || content.type === 'image'));

      if (group) {
        const previous = stepContent.at(-1);
        if (previous && (previous.type !== content.type || content.type === 'toolRequest')) {
          flushStep();
        }
        if (stepContent.length === 0) stepKey = contentIndex;
        stepContent.push(content);
        if (hasMcpApp) inlineContent.push(content);
      } else {
        flushStep();
        inlineContent.push(content);
        if (finalTextIndex === undefined && finalContent && isAnswerContent(content))
          footerIndex = index;
      }
    });
    flushStep();
    if (
      inlineContent.some((content) => content.type !== 'text' || content.text.trim()) ||
      message.metadata.outputTokenLimitReached
    ) {
      inline.set(index, { ...message, content: inlineContent });
    }
  }

  footerIndex ??= lastAssistantIndex;
  if (footerIndex !== undefined && !inline.has(footerIndex)) {
    const footerMessage = rows.find(({ index }) => index === footerIndex)?.message;
    if (footerMessage) inline.set(footerIndex, { ...footerMessage, content: [] });
  }

  const running = unanswered
    .filter((tool) => !tool.state.isPending)
    .flatMap((tool) => toolLabel(tool) || []);
  return {
    start: rows[0].index,
    end: rows[rows.length - 1].index,
    active,
    toolCount: calls.length,
    status,
    runningTool: active && status !== 'waiting' ? running.at(-1) : undefined,
    lastTool: calls.length > 0 ? toolLabel(calls[calls.length - 1]) : undefined,
    steps,
    inline,
    footerIndex,
  };
}

export function deriveTurnActivity(
  visibleMessages: IndexedMessage[],
  rowContexts: MessageRowContext[],
  isUserMessage: (message: Message) => boolean,
  active: boolean,
  showThinking: boolean
): TurnActivity[] {
  const turns: IndexedMessage[][] = [];
  let rows: IndexedMessage[] = [];
  for (const row of visibleMessages) {
    if (rows.length > 0 && isTurnBoundary(row.message, isUserMessage)) {
      turns.push(rows);
      rows = [];
    }
    rows.push(row);
  }
  if (rows.length > 0) turns.push(rows);

  return turns.map((turnRows, index) =>
    describeTurn(turnRows, rowContexts, active && index === turns.length - 1, showThinking)
  );
}
