import type { SessionNotification } from '@agentclientprotocol/sdk';
import type { Message } from '../api';

export type AcpDesktopUpdate =
  | { type: 'messages'; messages: Message[] }
  | { type: 'sessionInfo'; name?: string };

interface AdapterState {
  messages: Message[];
  fallbackMessageCount: number;
}

export interface AcpSessionNotificationAdapter {
  apply(notification: SessionNotification): AcpDesktopUpdate[];
  getMessages(): Message[];
}

export function createAcpSessionNotificationAdapter(
  initialMessages: Message[] = []
): AcpSessionNotificationAdapter {
  const state: AdapterState = {
    messages: [...initialMessages],
    fallbackMessageCount: 0,
  };

  return {
    apply(notification) {
      return applyAcpSessionNotification(state, notification);
    },
    getMessages() {
      return [...state.messages];
    },
  };
}

function applyAcpSessionNotification(
  state: AdapterState,
  notification: SessionNotification
): AcpDesktopUpdate[] {
  const { update } = notification;

  switch (update.sessionUpdate) {
    case 'user_message_chunk':
      return applyTextChunk(state, 'user', update);

    case 'agent_message_chunk':
      return applyTextChunk(state, 'assistant', update);

    case 'session_info_update':
      return [{ type: 'sessionInfo', name: update.title ?? undefined }];

    default:
      return [];
  }
}

function applyTextChunk(
  state: AdapterState,
  role: Message['role'],
  update: Extract<
    SessionNotification['update'],
    { sessionUpdate: 'user_message_chunk' | 'agent_message_chunk' }
  >
): AcpDesktopUpdate[] {
  if (update.content.type !== 'text') {
    return [];
  }

  const id = update.messageId ?? nextFallbackMessageId(state, role);
  const existing = state.messages.find((message) => message.id === id && message.role === role);

  if (existing) {
    const lastContent = existing.content[existing.content.length - 1];
    if (lastContent?.type === 'text') {
      lastContent.text += update.content.text;
    } else {
      existing.content.push({ type: 'text', text: update.content.text });
    }
  } else {
    state.messages.push({
      id,
      role,
      created: Math.floor(Date.now() / 1000),
      content: [{ type: 'text', text: update.content.text }],
      metadata: { userVisible: true, agentVisible: true },
    });
  }

  return [{ type: 'messages', messages: [...state.messages] }];
}

function nextFallbackMessageId(state: AdapterState, role: Message['role']): string {
  state.fallbackMessageCount += 1;
  return `acp-${role}-${state.fallbackMessageCount}`;
}
