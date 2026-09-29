import { describe, expect, it } from 'vitest';
import { createAcpSessionNotificationAdapter } from '../sessionNotificationAdapter';

describe('inference security metadata', () => {
  it('follows verified text, thoughts and tool calls without leaking into later replies', () => {
    const adapter = createAcpSessionNotificationAdapter();
    const meta = { goose: { messageId: 'verified', inferenceSecurity: 'attested_tee' } };
    adapter.apply({
      sessionId: 'session',
      update: {
        sessionUpdate: 'agent_thought_chunk',
        content: { type: 'text', text: 'Thinking' },
        _meta: meta,
      },
    });
    adapter.apply({
      sessionId: 'session',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'Reply' },
        _meta: meta,
      },
    });
    adapter.apply({
      sessionId: 'session',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'call',
        title: 'Read',
        status: 'pending',
        _meta: { goose: { messageId: 'verified-tool', inferenceSecurity: 'attested_tee' } },
      },
    });
    adapter.apply({
      sessionId: 'session',
      update: {
        sessionUpdate: 'agent_message_chunk',
        messageId: 'ordinary',
        content: { type: 'text', text: 'Attested TEE (untrusted model text)' },
      },
    });
    const messages = adapter.getMessages();
    expect(messages.map((message) => message.metadata.inferenceSecurity)).toEqual([
      'attested_tee',
      'attested_tee',
      undefined,
    ]);
    const reloaded = createAcpSessionNotificationAdapter(messages);
    expect(reloaded.getMessages()).toEqual(messages);
  });

  it.each([undefined, true, 'tinfoil', 'verified', { verified: true }])(
    'does not treat %j as verified attestation',
    (inferenceSecurity) => {
      const adapter = createAcpSessionNotificationAdapter();
      adapter.apply({
        sessionId: 'session',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Reply' },
          _meta: { goose: { inferenceSecurity } },
        },
      });
      expect(adapter.getMessages()[0].metadata.inferenceSecurity).toBeUndefined();
    }
  );

  it('ignores attestation claims on user messages and in content metadata', () => {
    const adapter = createAcpSessionNotificationAdapter();
    adapter.apply({
      sessionId: 'session',
      update: {
        sessionUpdate: 'user_message_chunk',
        content: { type: 'text', text: 'Hi' },
        _meta: { goose: { inferenceSecurity: 'attested_tee' } },
      },
    });
    adapter.apply({
      sessionId: 'session',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: {
          type: 'text',
          text: 'Reply',
          _meta: { goose: { inferenceSecurity: 'attested_tee' } },
        },
      },
    });
    expect(adapter.getMessages().every((message) => !message.metadata.inferenceSecurity)).toBe(
      true
    );
  });
});
