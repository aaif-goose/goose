import type { SessionNotification } from '@agentclientprotocol/sdk';
import { describe, expect, it } from 'vitest';
import { createAcpSessionNotificationAdapter } from '../sessionNotificationAdapter';

function textNotification(
  sessionUpdate: 'user_message_chunk' | 'agent_message_chunk',
  text: string,
  messageId = 'message-1'
): SessionNotification {
  return {
    sessionId: 'session-1',
    update: {
      sessionUpdate,
      messageId,
      content: {
        type: 'text',
        text,
      },
    },
  } as SessionNotification;
}

describe('sessionNotificationAdapter', () => {
  it('converts a user text chunk into a desktop message', () => {
    const adapter = createAcpSessionNotificationAdapter();

    const updates = adapter.apply(textNotification('user_message_chunk', 'hello'));

    expect(updates).toEqual([
      {
        type: 'messages',
        messages: [
          {
            id: 'message-1',
            role: 'user',
            created: expect.any(Number),
            content: [{ type: 'text', text: 'hello' }],
            metadata: { userVisible: true, agentVisible: true },
          },
        ],
      },
    ]);
  });

  it('converts an agent text chunk into an assistant desktop message', () => {
    const adapter = createAcpSessionNotificationAdapter();

    adapter.apply(textNotification('agent_message_chunk', 'hi'));

    expect(adapter.getMessages()).toMatchObject([
      {
        id: 'message-1',
        role: 'assistant',
        content: [{ type: 'text', text: 'hi' }],
      },
    ]);
  });

  it('appends text chunks with the same role and message ID', () => {
    const adapter = createAcpSessionNotificationAdapter();

    adapter.apply(textNotification('agent_message_chunk', 'hello ', 'message-1'));
    adapter.apply(textNotification('agent_message_chunk', 'there', 'message-1'));

    expect(adapter.getMessages()).toMatchObject([
      {
        id: 'message-1',
        role: 'assistant',
        content: [{ type: 'text', text: 'hello there' }],
      },
    ]);
  });

  it('keeps different message IDs as separate messages', () => {
    const adapter = createAcpSessionNotificationAdapter();

    adapter.apply(textNotification('agent_message_chunk', 'first', 'message-1'));
    adapter.apply(textNotification('agent_message_chunk', 'second', 'message-2'));

    expect(adapter.getMessages()).toMatchObject([
      {
        id: 'message-1',
        content: [{ type: 'text', text: 'first' }],
      },
      {
        id: 'message-2',
        content: [{ type: 'text', text: 'second' }],
      },
    ]);
  });

  it('converts session info title updates', () => {
    const adapter = createAcpSessionNotificationAdapter();

    const updates = adapter.apply({
      sessionId: 'session-1',
      update: {
        sessionUpdate: 'session_info_update',
        title: 'New title',
      },
    } as SessionNotification);

    expect(updates).toEqual([{ type: 'sessionInfo', name: 'New title' }]);
  });

  it('ignores non-text content for the minimal text adapter', () => {
    const adapter = createAcpSessionNotificationAdapter();

    const updates = adapter.apply({
      sessionId: 'session-1',
      update: {
        sessionUpdate: 'agent_message_chunk',
        messageId: 'message-1',
        content: {
          type: 'image',
          data: 'abc',
          mimeType: 'image/png',
        },
      },
    } as SessionNotification);

    expect(updates).toEqual([]);
    expect(adapter.getMessages()).toEqual([]);
  });
});
