import { describe, expect, it } from 'vitest';
import { reasoningConsumedOutputBudget, type Message } from './message';

function assistant(partial: Partial<Message> & Pick<Message, 'content'>): Message {
  return {
    created: 1,
    role: 'assistant',
    metadata: {
      agentVisible: true,
      userVisible: true,
      ...partial.metadata,
    },
    ...partial,
  };
}

describe('reasoningConsumedOutputBudget', () => {
  it('uses preceding thinking when the length marker has a generated id', () => {
    const messages: Message[] = [
      assistant({
        id: 'msg_thinking',
        content: [{ type: 'thinking', thinking: 'internal reasoning', signature: '' }],
      }),
      assistant({
        id: 'msg_marker',
        content: [],
        metadata: {
          agentVisible: false,
          userVisible: true,
          outputTokenLimitReached: true,
          fallbackContent: true,
        },
      }),
    ];

    expect(reasoningConsumedOutputBudget(messages, 1)).toBe(true);
  });

  it('does not treat a later length hit as reasoning after a tool call', () => {
    const messages: Message[] = [
      assistant({
        id: 'msg_thinking',
        content: [{ type: 'thinking', thinking: 'previous call reasoning', signature: '' }],
      }),
      assistant({
        id: 'msg_tool',
        content: [
          {
            type: 'toolRequest',
            id: 'call-1',
            toolCall: {
              name: 'shell',
              arguments: { command: 'echo hi' },
            },
          },
        ],
      }),
      assistant({
        id: 'msg_marker',
        content: [],
        metadata: {
          agentVisible: false,
          userVisible: true,
          outputTokenLimitReached: true,
        },
      }),
    ];

    expect(reasoningConsumedOutputBudget(messages, 2)).toBe(false);
  });

  it('uses preceding thinking when chunk ids differ', () => {
    const messages: Message[] = [
      assistant({
        id: 'chatcmpl-1',
        content: [{ type: 'thinking', thinking: 'internal reasoning', signature: '' }],
      }),
      assistant({
        id: 'chatcmpl-2',
        content: [],
        metadata: {
          agentVisible: false,
          userVisible: true,
          outputTokenLimitReached: true,
        },
      }),
    ];

    expect(reasoningConsumedOutputBudget(messages, 1)).toBe(true);
  });

  it('ignores ACP fallback text on the length marker', () => {
    const messages: Message[] = [
      assistant({
        id: 'chatcmpl-1',
        content: [{ type: 'thinking', thinking: 'internal reasoning', signature: '' }],
      }),
      assistant({
        id: 'chatcmpl-2',
        content: [
          {
            type: 'text',
            text: 'Response stopped because the model reached its output-token limit.',
          },
        ],
        metadata: {
          agentVisible: false,
          userVisible: true,
          outputTokenLimitReached: true,
          fallbackContent: true,
        },
      }),
    ];

    expect(reasoningConsumedOutputBudget(messages, 1)).toBe(true);
  });

  it('treats redacted thinking as reasoning', () => {
    const messages: Message[] = [
      assistant({
        id: 'msg_thinking',
        content: [{ type: 'redactedThinking', data: 'redacted' }],
      }),
      assistant({
        id: 'msg_marker',
        content: [],
        metadata: {
          agentVisible: false,
          userVisible: true,
          outputTokenLimitReached: true,
        },
      }),
    ];

    expect(reasoningConsumedOutputBudget(messages, 1)).toBe(true);
  });
});
