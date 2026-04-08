import {
  Message,
  MessageEvent,
  ActionRequired,
  ToolRequest,
  ToolResponse,
  ToolConfirmationRequest,
} from '../api';

export type ToolRequestMessageContent = ToolRequest & { type: 'toolRequest' };
export type ToolResponseMessageContent = ToolResponse & { type: 'toolResponse' };
export type ToolConfirmationRequestContent = ToolConfirmationRequest & {
  type: 'toolConfirmationRequest';
};
export type NotificationEvent = Extract<MessageEvent, { type: 'Notification' }>;

// Compaction response message - must match backend constant
const COMPACTION_THINKING_TEXT = 'goose is compacting the conversation...';

export interface ImageData {
  data: string; // base64 encoded image data
  mimeType: string;
}

export interface UserInput {
  msg: string;
  images: ImageData[];
}

export function createUserMessage(text: string, images?: ImageData[]): Message {
  const content: Message['content'] = [];

  if (text.trim()) {
    content.push({ type: 'text', text });
  }

  if (images && images.length > 0) {
    images.forEach((img) => {
      content.push({
        type: 'image',
        data: img.data,
        mimeType: img.mimeType,
      });
    });
  }

  return {
    id: generateMessageId(),
    role: 'user',
    created: Math.floor(Date.now() / 1000),
    content,
    metadata: { userVisible: true, agentVisible: true },
  };
}

export function createElicitationResponseMessage(
  elicitationId: string,
  userData: Record<string, unknown>
): Message {
  return {
    id: generateMessageId(),
    role: 'user',
    created: Math.floor(Date.now() / 1000),
    content: [
      {
        type: 'actionRequired',
        data: {
          actionType: 'elicitationResponse',
          id: elicitationId,
          user_data: userData,
        },
      },
    ],
    metadata: { userVisible: false, agentVisible: true },
  };
}

export function generateMessageId(): string {
  return Math.random().toString(36).substring(2, 10);
}

export function getTextAndImageContent(message: Message): {
  textContent: string;
  imagePaths: string[];
} {
  let textContent = '';
  const imagePaths: string[] = [];

  for (const content of message.content) {
    if (content.type === 'text') {
      textContent += content.text;
    } else if (content.type === 'image') {
      imagePaths.push(`data:${content.mimeType};base64,${content.data}`);
    }
  }

  // Strip <think> tags from assistant text — the thinking is surfaced via getThinkingContent
  if (message.role === 'assistant') {
    textContent = textContent.replace(/<think>[\s\S]*?<\/think>/gi, '');
  }

  return { textContent, imagePaths };
}

export function getThinkingContent(message: Message): string | null {
  const parts: string[] = [];

  // Structured thinking content blocks
  for (const content of message.content) {
    if (content.type === 'thinking' && 'thinking' in content && content.thinking) {
      parts.push(content.thinking);
    }
  }

  // Inline <think> tags in assistant text content
  if (message.role === 'assistant') {
    for (const content of message.content) {
      if (content.type === 'text') {
        const regex = /<think>([\s\S]*?)<\/think>/gi;
        let match;
        while ((match = regex.exec(content.text)) !== null) {
          const text = match[1].trim();
          if (text) parts.push(text);
        }
      }
    }
  }

  return parts.length > 0 ? parts.join('') : null;
}

export function getToolRequests(message: Message): (ToolRequest & { type: 'toolRequest' })[] {
  return message.content.filter(
    (content): content is ToolRequest & { type: 'toolRequest' } => content.type === 'toolRequest'
  );
}

export function getToolResponses(message: Message): (ToolResponse & { type: 'toolResponse' })[] {
  return message.content.filter(
    (content): content is ToolResponse & { type: 'toolResponse' } => content.type === 'toolResponse'
  );
}

export function getToolConfirmationContent(
  message: Message
): (ActionRequired & { type: 'actionRequired' }) | undefined {
  return message.content.find(
    (content): content is ActionRequired & { type: 'actionRequired' } =>
      content.type === 'actionRequired' && content.data.actionType === 'toolConfirmation'
  );
}

export function getToolConfirmationRequestContent(
  message: Message
): ToolConfirmationRequestContent | undefined {
  return message.content.find(
    (content): content is ToolConfirmationRequestContent =>
      content.type === 'toolConfirmationRequest'
  );
}

export interface ToolConfirmationData {
  id: string;
  toolName: string;
  arguments: Record<string, unknown>;
  prompt?: string | null;
}

export function getAnyToolConfirmationData(message: Message): ToolConfirmationData | undefined {
  const confirmationRequest = getToolConfirmationRequestContent(message);
  if (confirmationRequest) {
    return {
      id: confirmationRequest.id,
      toolName: confirmationRequest.toolName,
      arguments: confirmationRequest.arguments,
      prompt: confirmationRequest.prompt,
    };
  }

  const actionRequired = getToolConfirmationContent(message);
  if (actionRequired && actionRequired.data.actionType === 'toolConfirmation') {
    return {
      id: actionRequired.data.id,
      toolName: actionRequired.data.toolName,
      arguments: actionRequired.data.arguments,
      prompt: actionRequired.data.prompt,
    };
  }

  return undefined;
}

export function getToolConfirmationId(
  content: ActionRequired & { type: 'actionRequired' }
): string | undefined {
  if (content.data.actionType === 'toolConfirmation') {
    return content.data.id;
  }
  return undefined;
}

export function getPendingToolConfirmationIds(messages: Message[]): Set<string> {
  const pendingIds = new Set<string>();
  const respondedIds = new Set<string>();

  for (const message of messages) {
    const responses = getToolResponses(message);
    for (const response of responses) {
      respondedIds.add(response.id);
    }
  }

  for (const message of messages) {
    const confirmationData = getAnyToolConfirmationData(message);
    if (confirmationData && !respondedIds.has(confirmationData.id)) {
      pendingIds.add(confirmationData.id);
    }
  }

  return pendingIds;
}

export function getElicitationContent(
  message: Message
): (ActionRequired & { type: 'actionRequired' }) | undefined {
  return message.content.find(
    (content): content is ActionRequired & { type: 'actionRequired' } =>
      content.type === 'actionRequired' && content.data.actionType === 'elicitation'
  );
}

export function hasCompletedToolCalls(message: Message): boolean {
  const toolRequests = getToolRequests(message);
  return toolRequests.length > 0;
}

export function getThinkingMessage(message: Message | undefined): string | undefined {
  if (!message || message.role !== 'assistant') {
    return undefined;
  }

  for (const content of message.content) {
    if (content.type === 'systemNotification' && content.notificationType === 'thinkingMessage') {
      return content.msg;
    }
  }

  return undefined;
}

export function getCompactingMessage(message: Message | undefined): string | undefined {
  if (!message || message.role !== 'assistant') {
    return undefined;
  }

  for (const content of message.content) {
    if (content.type === 'systemNotification' && content.notificationType === 'thinkingMessage') {
      if (content.msg === COMPACTION_THINKING_TEXT) {
        return content.msg;
      }
    }
  }

  return undefined;
}
