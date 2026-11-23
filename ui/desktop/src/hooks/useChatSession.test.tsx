import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  snapshot: undefined as unknown,
  submitMessage: vi.fn(),
}));

vi.mock('../acp/chatSessionController', () => ({
  acpChatSessionController: {
    submitMessage: mocks.submitMessage,
    loadSession: vi.fn(),
    restoreSession: vi.fn(),
  },
}));

vi.mock('../acp/chatSessionStore', () => ({
  useAcpChatSessionSnapshot: () => mocks.snapshot,
  acpChatSessionStore: { getSnapshot: () => mocks.snapshot },
  acpChatSessionActions: { setMessages: vi.fn() },
}));

vi.mock('../acp/acpConnection', () => ({
  isAcpRecovering: () => false,
}));

import { useChatSession } from './useChatSession';
import { IntlTestWrapper } from '../i18n/test-utils';
import { ChatState } from '../types/chatState';
import { createUserMessage, type Message } from '../types/message';

function assistantMessage(text: string): Message {
  return {
    id: 'assistant-1',
    role: 'assistant',
    created: 0,
    content: [{ type: 'text', text }],
    metadata: { userVisible: true, agentVisible: true },
  };
}

function renderChatSession(messages: Message[]) {
  mocks.snapshot = {
    session: { id: 'sess-1' },
    messages,
    chatState: ChatState.Idle,
    pendingCancelPromptAttemptId: null,
    activeRunId: null,
  };
  return renderHook(() => useChatSession({ sessionId: 'sess-1', onStreamFinish: vi.fn() }), {
    wrapper: IntlTestWrapper,
  });
}

describe('useChatSession handleSubmit', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('does not send the last assistant message as a new prompt on an empty submit', async () => {
    const { result } = renderChatSession([
      createUserMessage('Question'),
      assistantMessage('Answer'),
    ]);

    await act(() => result.current.handleSubmit({ msg: '', images: [] }));

    expect(mocks.submitMessage).not.toHaveBeenCalled();
  });

  it('sends the last user message again on an empty submit', async () => {
    const question = createUserMessage('Question');
    const { result } = renderChatSession([question]);

    await act(() => result.current.handleSubmit({ msg: '', images: [] }));

    expect(mocks.submitMessage).toHaveBeenCalledTimes(1);
    expect(mocks.submitMessage).toHaveBeenCalledWith('sess-1', question, expect.anything());
  });
});
